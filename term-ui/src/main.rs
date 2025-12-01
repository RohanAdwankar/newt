use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyboardEnhancementFlags, PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use directories::ProjectDirs;
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{error::Error, io, process::Command, path::PathBuf, fs};
use tempfile::NamedTempFile;
use std::io::Write;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Open a specific notebook file or the file menu
    #[arg(short, long, num_args=0..=1, default_missing_value = "")]
    open: Option<String>,
}

#[derive(Serialize)]
struct CommandRequest {
    command: String,
    language: Option<String>,
}

#[derive(Deserialize)]
struct CommandResponse {
    stdout: String,
    stderr: String,
    #[allow(dead_code)]
    status: Option<i32>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
enum CellType {
    Shell,
    Rust,
}

#[derive(Clone, Serialize, Deserialize)]
struct Cell {
    id: String,
    content: String,
    output: String,
    cell_type: CellType,
}

struct App {
    cells: Vec<Cell>,
    list_state: ListState,
    input: String,
    input_mode: InputMode,
    pending_delete: bool,
    command_input: String,
    app_mode: AppMode,
    file_path: Option<PathBuf>,
    file_list_state: ListState,
    available_files: Vec<PathBuf>,
}

#[derive(PartialEq)]
enum AppMode {
    Editor,
    FileMenu,
}

enum InputMode {
    Normal,
    Editing,
    Command,
}

impl App {
    fn new(open_arg: Option<String>) -> App {
        let mut app = App {
            cells: Vec::new(),
            list_state: ListState::default(),
            input: String::new(),
            input_mode: InputMode::Editing,
            pending_delete: false,
            command_input: String::new(),
            app_mode: AppMode::Editor,
            file_path: None,
            file_list_state: ListState::default(),
            available_files: Vec::new(),
        };

        match open_arg {
            Some(path_str) => {
                if path_str.is_empty() {
                    // --open with no args: File Menu
                    app.app_mode = AppMode::FileMenu;
                    app.refresh_file_list();
                } else {
                    // -o path: Open file
                    let path = PathBuf::from(&path_str);
                    if path.exists() {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(cells) = serde_json::from_str(&content) {
                                app.cells = cells;
                                app.file_path = Some(path);
                                app.input_mode = InputMode::Normal;
                                app.list_state.select(Some(0));
                                return app;
                            }
                        }
                    }
                    // If file doesn't exist or fails to load, start empty but set path
                    app.file_path = Some(path);
                    app.add_cell(CellType::Shell);
                }
            }
            None => {
                // No args: New notebook
                app.add_cell(CellType::Shell);
            }
        }
        app
    }

    fn refresh_file_list(&mut self) {
        self.available_files.clear();
        let dir = get_app_dir();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "newt") {
                    self.available_files.push(path);
                }
            }
        }
        // Sort files
        self.available_files.sort();
        if !self.available_files.is_empty() {
            self.file_list_state.select(Some(0));
        }
    }

    fn add_cell(&mut self, cell_type: CellType) {
        self.insert_cell(self.cells.len(), cell_type);
        self.input_mode = InputMode::Editing;
    }

    fn insert_cell(&mut self, index: usize, cell_type: CellType) {
        let id = uuid::Uuid::new_v4().to_string();
        self.cells.insert(index, Cell {
            id,
            content: String::new(),
            output: String::new(),
            cell_type,
        });
        self.list_state.select(Some(index));
        self.input.clear();
        self.input_mode = InputMode::Normal;
    }

    fn delete_current_cell(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if self.cells.len() > 0 {
                self.cells.remove(i);
                if self.cells.is_empty() {
                    // Always keep at least one cell
                    self.add_cell(CellType::Shell);
                } else if i >= self.cells.len() {
                    self.list_state.select(Some(self.cells.len() - 1));
                }
            }
        }
    }

    fn current_cell_mut(&mut self) -> Option<&mut Cell> {
        if let Some(i) = self.list_state.selected() {
            self.cells.get_mut(i)
        } else {
            None
        }
    }

    fn save_notebook(&mut self, filename: Option<&str>) -> io::Result<()> {
        let path = if let Some(name) = filename {
             PathBuf::from(name)
        } else if let Some(ref p) = self.file_path {
             p.clone()
        } else {
             let mut p = get_app_dir();
             p.push("notebook.newt");
             p
        };
        
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(&self.cells)?;
        fs::write(&path, json)?;
        self.file_path = Some(path);
        Ok(())
    }

    fn get_run_request(&self, index: usize) -> Option<CommandRequest> {
        if let Some(cell) = self.cells.get(index) {
            let cmd = cell.content.clone();
            let lang = match cell.cell_type {
                CellType::Rust => Some("rust".to_string()),
                CellType::Shell => None,
            };
            Some(CommandRequest { command: cmd, language: lang })
        } else {
            None
        }
    }
    
    fn update_cell_output(&mut self, index: usize, output: String) {
        if let Some(cell) = self.cells.get_mut(index) {
            cell.output = output;
        }
    }
}

fn get_app_dir() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("com", "newt", "newt") {
        let data_dir = proj_dirs.data_dir();
        if !data_dir.exists() {
            std::fs::create_dir_all(data_dir).unwrap_or_default();
        }
        data_dir.to_path_buf()
    } else {
        PathBuf::from(".")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout, 
        EnterAlternateScreen, 
        EnableMouseCapture,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(args.open);

    // Run app
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        PopKeyboardEnhancementFlags
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

async fn run_app<B: Backend + std::io::Write>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    let client = reqwest::Client::new();

    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app.app_mode {
                    AppMode::FileMenu => {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                            KeyCode::Char('j') | KeyCode::Down => {
                                if let Some(i) = app.file_list_state.selected() {
                                    if i < app.available_files.len() { // +1 for "New Notebook" but index 0 is new
                                        app.file_list_state.select(Some(i + 1));
                                    }
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                if let Some(i) = app.file_list_state.selected() {
                                    if i > 0 {
                                        app.file_list_state.select(Some(i - 1));
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                if let Some(i) = app.file_list_state.selected() {
                                    if i == 0 {
                                        // New Notebook
                                        app.cells.clear();
                                        app.add_cell(CellType::Shell);
                                        app.file_path = None;
                                        app.app_mode = AppMode::Editor;
                                        app.input_mode = InputMode::Normal;
                                        app.list_state.select(Some(0));
                                    } else {
                                        // Open selected file
                                        if let Some(path) = app.available_files.get(i - 1) {
                                            if let Ok(content) = fs::read_to_string(path) {
                                                if let Ok(cells) = serde_json::from_str(&content) {
                                                    app.cells = cells;
                                                    app.file_path = Some(path.clone());
                                                    app.app_mode = AppMode::Editor;
                                                    app.input_mode = InputMode::Normal;
                                                    app.list_state.select(Some(0));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    AppMode::Editor => {
                        match app.input_mode {
                            InputMode::Normal => match key.code {
                                KeyCode::Char(':') => {
                                    app.input_mode = InputMode::Command;
                                    app.command_input.clear();
                                }
                                KeyCode::Char('j') => {
                                    if let Some(i) = app.list_state.selected() {
                                        if i < app.cells.len() - 1 {
                                            app.list_state.select(Some(i + 1));
                                        }
                                    }
                                }
                                KeyCode::Char('k') => {
                                    if let Some(i) = app.list_state.selected() {
                                        if i > 0 {
                                            app.list_state.select(Some(i - 1));
                                        }
                                    }
                                }
                                KeyCode::Char('o') => {
                                    if let Some(i) = app.list_state.selected() {
                                        app.insert_cell(i + 1, CellType::Shell);
                                    }
                                }
                                KeyCode::Char('O') => {
                                    if let Some(i) = app.list_state.selected() {
                                        app.insert_cell(i, CellType::Shell);
                                    }
                                }
                                KeyCode::Char('d') => {
                                    if app.pending_delete {
                                        app.delete_current_cell();
                                        app.pending_delete = false;
                                    } else {
                                        app.pending_delete = true;
                                    }
                                }
                                KeyCode::Char('i') => {
                                    app.pending_delete = false;
                                    // Edit current cell
                                    if let Some(i) = app.list_state.selected() {
                                        let cell = &app.cells[i];
                                        match cell.cell_type {
                                            CellType::Shell => {
                                                app.input = cell.content.clone();
                                                app.input_mode = InputMode::Editing;
                                            }
                                            CellType::Rust => {
                                                // Open editor
                                                let content = cell.content.clone();
                                                
                                                // Suspend TUI
                                                disable_raw_mode()?;
                                                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                                
                                                let new_content = open_editor(&content)?;
                                                
                                                // Resume TUI
                                                execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                                                enable_raw_mode()?;
                                                terminal.clear()?; // Force redraw

                                                if let Some(cell) = app.current_cell_mut() {
                                                    cell.content = new_content;
                                                }
                                            }
                                        }
                                    }
                                }
                                KeyCode::Enter => {
                                     app.pending_delete = false;
                                     // Run the selected cell
                                     if let Some(i) = app.list_state.selected() {
                                        if let Some(req) = app.get_run_request(i) {
                                            let client = client.clone();
                                            let res = client.post("http://127.0.0.1:3000/exec")
                                                .json(&req)
                                                .send()
                                                .await;

                                            match res {
                                                Ok(resp) => {
                                                    if let Ok(body) = resp.json::<CommandResponse>().await {
                                                        app.update_cell_output(i, format!("{}{}", body.stdout, body.stderr));
                                                    } else {
                                                        app.update_cell_output(i, "Error parsing response".to_string());
                                                    }
                                                }
                                                Err(e) => {
                                                    app.update_cell_output(i, format!("Error connecting to server: {}", e));
                                                }
                                            }
                                        }
                                     }
                                }
                                KeyCode::Esc => {
                                    app.pending_delete = false;
                                }
                                _ => {
                                    app.pending_delete = false;
                                }
                            },
                            InputMode::Command => match key.code {
                                KeyCode::Enter => {
                                    match app.command_input.as_str() {
                                        "q" => return Ok(()),
                                        "w" => {
                                            app.save_notebook(None)?;
                                            app.input_mode = InputMode::Normal;
                                        }
                                        "wq" => {
                                            app.save_notebook(None)?;
                                            return Ok(());
                                        }
                                        "ra" | "runall" => {
                                            app.input_mode = InputMode::Normal;
                                            // Run all cells
                                            for i in 0..app.cells.len() {
                                                if let Some(req) = app.get_run_request(i) {
                                                    let client = client.clone();
                                                    let res = client.post("http://127.0.0.1:3000/exec")
                                                        .json(&req)
                                                        .send()
                                                        .await;

                                                    match res {
                                                        Ok(resp) => {
                                                            if let Ok(body) = resp.json::<CommandResponse>().await {
                                                                app.update_cell_output(i, format!("{}{}", body.stdout, body.stderr));
                                                            } else {
                                                                app.update_cell_output(i, "Error parsing response".to_string());
                                                            }
                                                        }
                                                        Err(e) => {
                                                            app.update_cell_output(i, format!("Error connecting to server: {}", e));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        _ => {
                                            app.input_mode = InputMode::Normal;
                                        }
                                    }
                                    app.command_input.clear();
                                }
                                KeyCode::Char(c) => {
                                    app.command_input.push(c);
                                }
                                KeyCode::Backspace => {
                                    app.command_input.pop();
                                }
                                KeyCode::Esc => {
                                    app.input_mode = InputMode::Normal;
                                    app.command_input.clear();
                                }
                                _ => {}
                            },
                            InputMode::Editing => match key.code {
                                KeyCode::Enter => {
                                    // If typing "rust", switch to rust cell and open editor
                                    if app.input.trim() == "rust" {
                                        if let Some(cell) = app.current_cell_mut() {
                                            cell.cell_type = CellType::Rust;
                                            cell.content = String::new(); // Clear "rust"
                                            
                                            // Open editor
                                            disable_raw_mode()?;
                                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                            
                                            let new_content = open_editor(&cell.content)?;
                                            
                                            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                                            enable_raw_mode()?;
                                            terminal.clear()?;

                                            cell.content = new_content;
                                            app.input_mode = InputMode::Normal; 
                                        }
                                    } else {
                                        // Run the cell
                                        let input_content = app.input.clone();
                                        if let Some(cell) = app.current_cell_mut() {
                                            if cell.cell_type == CellType::Shell {
                                                cell.content = input_content;
                                            }
                                        }
                                        
                                        if let Some(i) = app.list_state.selected() {
                                            if let Some(req) = app.get_run_request(i) {
                                                let client = client.clone();
                                                let res = client.post("http://127.0.0.1:3000/exec")
                                                    .json(&req)
                                                    .send()
                                                    .await;

                                                match res {
                                                    Ok(resp) => {
                                                        if let Ok(body) = resp.json::<CommandResponse>().await {
                                                            app.update_cell_output(i, format!("{}{}", body.stdout, body.stderr));
                                                        } else {
                                                            app.update_cell_output(i, "Error parsing response".to_string());
                                                        }
                                                    }
                                                    Err(e) => {
                                                        app.update_cell_output(i, format!("Error connecting to server: {}", e));
                                                    }
                                                }
                                            }
                                        }
                                        
                                        if let Some(i) = app.list_state.selected() {
                                            if i == app.cells.len() - 1 {
                                                app.add_cell(CellType::Shell);
                                            } else {
                                                app.list_state.select(Some(i + 1));
                                                app.input_mode = InputMode::Normal;
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char(c) => {
                                    app.input.push(c);
                                }
                                KeyCode::Backspace => {
                                    app.input.pop();
                                }
                                KeyCode::Esc => {
                                    app.input_mode = InputMode::Normal;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
}

fn open_editor(content: &str) -> io::Result<String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    
    let mut file = NamedTempFile::new()?;
    write!(file, "{}", content)?;
    
    let path = file.path().to_str().unwrap().to_string();
    
    let status = Command::new(&editor)
        .arg(&path)
        .status()?;
        
    if !status.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "Editor exited with error"));
    }
    
    let new_content = std::fs::read_to_string(&path)?;
    Ok(new_content)
}

fn ui(f: &mut Frame, app: &App) {
    match app.app_mode {
        AppMode::FileMenu => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Percentage(100)].as_ref())
                .split(f.area());
            
            let mut items = vec![ListItem::new("New Notebook").style(Style::default().add_modifier(Modifier::BOLD))];
            for path in &app.available_files {
                items.push(ListItem::new(path.file_name().unwrap().to_string_lossy()));
            }
            
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Select Notebook"))
                .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
                
            f.render_stateful_widget(list, chunks[0], &mut app.file_list_state.clone());
        }
        AppMode::Editor => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
                .split(f.area());

            let cells: Vec<ListItem> = app
                .cells
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    let header = match cell.cell_type {
                        CellType::Shell => "Shell",
                        CellType::Rust => "Rust",
                    };
                    
                    let content = if cell.content.is_empty() {
                        "(empty)"
                    } else {
                        &cell.content
                    };

                    let style = if Some(i) == app.list_state.selected() {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };

                    let mut lines = vec![
                        Line::from(Span::styled(format!("[{}] {}", header, cell.id), style)),
                        Line::from(format!("In: {}", content)),
                    ];
                    
                    if !cell.output.is_empty() {
                        lines.push(Line::from("Out:"));
                        for line in cell.output.lines() {
                            lines.push(Line::from(format!("  {}", line)));
                        }
                    }
                    lines.push(Line::from("")); // Spacer

                    ListItem::new(lines)
                })
                .collect();

            let list = List::new(cells)
                .block(Block::default().borders(Borders::NONE))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD));
                
            f.render_stateful_widget(list, chunks[0], &mut app.list_state.clone());

            // Input box / Command bar
            match app.input_mode {
                InputMode::Editing => {
                    if let Some(i) = app.list_state.selected() {
                        if let Some(cell) = app.cells.get(i) {
                            if cell.cell_type == CellType::Shell {
                                let area = f.area();
                                let input_area = Rect::new(area.x, area.height.saturating_sub(3), area.width, 3);
                                f.render_widget(ratatui::widgets::Clear, input_area);
                                
                                let input_block = Paragraph::new(app.input.as_str())
                                    .style(Style::default().fg(Color::Yellow))
                                    .block(Block::default().borders(Borders::ALL).title("Input"));
                                f.render_widget(input_block, input_area);
                            }
                        }
                    }
                }
                InputMode::Command => {
                    let input_block = Paragraph::new(format!(":{}", app.command_input))
                        .style(Style::default().fg(Color::Cyan));
                    f.render_widget(input_block, chunks[1]);
                }
                InputMode::Normal => {
                     let status = if let Some(path) = &app.file_path {
                         path.file_name().unwrap().to_string_lossy().to_string()
                     } else {
                         "[No Name]".to_string()
                     };
                     let status_block = Paragraph::new(status)
                        .style(Style::default().fg(Color::DarkGray));
                     f.render_widget(status_block, chunks[1]);
                }
            }
        }
    }
}
