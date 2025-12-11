use clap::Parser;
use arboard::Clipboard;
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
use std::io::Write;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Open a specific notebook file or the file menu
    #[arg(short, long, num_args=0..=1, default_missing_value = "")]
    open: Option<String>,
}

#[derive(Serialize)]
pub struct CommandRequest {
    pub command: String,
    pub language: Option<String>,
    pub context: Option<String>,
}

#[derive(Serialize)]
pub struct Notebook {
    pub cells: Vec<Cell>,
}

#[derive(Deserialize)]
struct ExportResponse {
    markdown: String,
}

#[derive(Deserialize)]
struct CommandResponse {
    stdout: String,
    stderr: String,
    display_data: Option<Vec<DisplayData>>,
}

#[derive(Deserialize, Debug)]
struct DisplayData {
    data: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum CellType {
    Shell,
    Rust,
    Python,
    JavaScript,
    TypeScript,
    C,
    Cpp,
    Go,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Cell {
    pub id: String,
    pub content: String,
    pub output: String,
    pub cell_type: CellType,
    #[serde(default)]
    pub polling_interval: Option<u64>, // Interval in seconds
    #[serde(default)]
    pub last_run: Option<u64>, // Timestamp in seconds
}

pub struct App {
    pub cells: Vec<Cell>,
    pub list_state: ListState,
    pub input: String,
    pub input_mode: InputMode,
    pub pending_delete: bool,
    pub command_input: String,
    pub file_path: Option<PathBuf>,
    pub file_list_state: ListState,
    pub available_files: Vec<PathBuf>,
    pub pending_key: Option<char>,
    pub show_sidebar: bool,
    pub focus: Focus,
    pub clipboard_cell: Option<Cell>,
    pub clipboard_file: Option<PathBuf>,
    pub status_message: Option<String>,
    pub rename_input: String,
    pub polling_input: String,
}

#[derive(PartialEq)]
pub enum Focus {
    Editor,
    Sidebar,
}

#[derive(PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
    Command,
    Renaming,
    Polling,
}

impl App {
    pub fn new(open_arg: Option<String>) -> App {
        let mut app = App {
            cells: Vec::new(),
            list_state: ListState::default(),
            input: String::new(),
            input_mode: InputMode::Editing,
            pending_delete: false,
            command_input: String::new(),
            file_path: None,
            file_list_state: ListState::default(),
            available_files: Vec::new(),
            pending_key: None,
            show_sidebar: false,
            focus: Focus::Editor,
            clipboard_cell: None,
            clipboard_file: None,
            status_message: None,
            rename_input: String::new(),
            polling_input: String::new(),
        };

        // Always refresh file list for sidebar
        app.refresh_file_list();

        match open_arg {
            Some(path_str) => {
                if !path_str.is_empty() {
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
                }
            }
            None => {}
        }
        
        // Default to new notebook if no cells loaded
        if app.cells.is_empty() {
            app.add_cell(CellType::Shell);
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
        // Always select the first item (New Notebook)
        self.file_list_state.select(Some(0));
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
            polling_interval: None,
            last_run: None,
        });
        // Select the input of the new cell
        self.list_state.select(Some(index * 2));
        self.input.clear();
        self.input_mode = InputMode::Normal;
    }

    fn delete_current_cell(&mut self) {
        if let Some(i) = self.list_state.selected() {
            let cell_idx = i / 2;
            if self.cells.len() > 0 {
                self.cells.remove(cell_idx);
                if self.cells.is_empty() {
                    // Always keep at least one cell
                    self.add_cell(CellType::Shell);
                } else {
                    // Select the previous cell or the same index if it exists
                    let new_idx = if cell_idx >= self.cells.len() {
                        self.cells.len() - 1
                    } else {
                        cell_idx
                    };
                    self.list_state.select(Some(new_idx * 2));
                }
            }
        }
    }

    fn current_cell_mut(&mut self) -> Option<&mut Cell> {
        if let Some(i) = self.list_state.selected() {
            let cell_idx = i / 2;
            self.cells.get_mut(cell_idx)
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
             
             // Check if file exists and increment name
             let mut counter = 2;
             while p.exists() {
                 p.set_file_name(format!("notebook{}.newt", counter));
                 counter += 1;
             }
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

    pub fn check_polling(&mut self) -> Vec<usize> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let mut indices_to_run = Vec::new();
        for (i, cell) in self.cells.iter().enumerate() {
            if let Some(interval) = cell.polling_interval {
                if let Some(last) = cell.last_run {
                    if now >= last + interval {
                        indices_to_run.push(i);
                    }
                } else {
                    indices_to_run.push(i);
                }
            }
        }
        
        // Update last_run for those we are about to run
        for &i in &indices_to_run {
            if let Some(cell) = self.cells.get_mut(i) {
                cell.last_run = Some(now);
            }
        }
        
        indices_to_run
    }

    pub fn get_run_request(&self, index: usize) -> Option<CommandRequest> {
        if let Some(cell) = self.cells.get(index) {
            let cmd = cell.content.clone();
            let lang = match cell.cell_type {
                CellType::Rust => Some("rust".to_string()),
                CellType::Python => Some("python".to_string()),
                CellType::JavaScript => Some("javascript".to_string()),
                CellType::TypeScript => Some("typescript".to_string()),
                CellType::C => Some("c".to_string()),
                CellType::Cpp => Some("cpp".to_string()),
                CellType::Go => Some("go".to_string()),
                CellType::Shell => None,
            };

            let mut context = String::new();
            if let Some(l) = &lang {
                for i in 0..index {
                    if let Some(prev_cell) = self.cells.get(i) {
                        let prev_lang = match prev_cell.cell_type {
                             CellType::Rust => Some("rust".to_string()),
                             CellType::Python => Some("python".to_string()),
                             CellType::JavaScript => Some("javascript".to_string()),
                             CellType::TypeScript => Some("typescript".to_string()),
                             CellType::C => Some("c".to_string()),
                             CellType::Cpp => Some("cpp".to_string()),
                             CellType::Go => Some("go".to_string()),
                             CellType::Shell => None,
                        };
                        if prev_lang.as_ref() == Some(l) {
                            context.push_str(&prev_cell.content);
                            context.push('\n');
                        }
                    }
                }
            }
            
            let context_opt = if context.is_empty() { None } else { Some(context) };

            Some(CommandRequest { command: cmd, language: lang, context: context_opt })
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
pub async fn run() -> Result<(), Box<dyn Error>> {
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
        // Check polling cells
        let indices_to_run = app.check_polling();

        for i in indices_to_run {
            if let Some(req) = app.get_run_request(i) {
                let client = client.clone();
                let res = client.post("http://127.0.0.1:3000/exec")
                    .json(&req)
                    .send()
                    .await;

                match res {
                    Ok(resp) => {
                        if let Ok(body) = resp.json::<CommandResponse>().await {
                            let mut output = format!("{}{}", body.stdout, body.stderr);
                            if let Some(display_data) = body.display_data {
                                for data in display_data {
                                    if let Some(image_path) = data.data.get("image/png").or(data.data.get("image/svg+xml")) {
                                        if let Some(path_str) = image_path.as_str() {
                                            output.push_str(&format!("\n[Image: {}]", path_str));
                                        }
                                    }
                                }
                            }
                            app.update_cell_output(i, output);
                        }
                    }
                    Err(e) => {
                        app.update_cell_output(i, format!("Error connecting to server: {}", e));
                    }
                }
            }
        }

        terminal.draw(|f| ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // 1. Check pending key for multi-key sequences
                if let Some(pending) = app.pending_key {
                    match (pending, key.code) {
                        (' ', KeyCode::Char('e')) => {
                            app.show_sidebar = !app.show_sidebar;
                            if app.show_sidebar {
                                app.focus = Focus::Sidebar;
                                app.refresh_file_list();
                            } else {
                                app.focus = Focus::Editor;
                            }
                            app.pending_key = None;
                            continue;
                        }
                        _ => {
                            app.pending_key = None;
                            // Fall through to handle the second key as a normal key if sequence failed?
                            // Or just consume it? Usually consume and reset.
                            // But if I type ' ' then 'a', maybe 'a' should be handled?
                            // For now, let's just reset and ignore the second key if it doesn't match.
                            // Or better, re-process the key if it wasn't part of a sequence?
                            // Let's keep it simple: if sequence fails, key is consumed.
                        }
                    }
                } else {
                    // Start sequence?
                    if app.input_mode == InputMode::Normal && key.code == KeyCode::Char(' ') {
                        app.pending_key = Some(' ');
                        continue;
                    }
                }

                // Handle keys based on Focus and InputMode
                match app.input_mode {
                    InputMode::Normal => {
                        match app.focus {
                            Focus::Sidebar => {
                                match key.code {
                                    KeyCode::Char('q') => return Ok(()),
                                    KeyCode::Char('j') | KeyCode::Down => {
                                        if let Some(i) = app.file_list_state.selected() {
                                            if i < app.available_files.len() { 
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
                                                app.input_mode = InputMode::Normal;
                                                app.list_state.select(Some(0));
                                            } else {
                                                // Open selected file
                                                if let Some(path) = app.available_files.get(i - 1) {
                                                    if let Ok(content) = fs::read_to_string(path) {
                                                        if let Ok(cells) = serde_json::from_str(&content) {
                                                            app.cells = cells;
                                                            app.file_path = Some(path.clone());
                                                            app.input_mode = InputMode::Normal;
                                                            app.list_state.select(Some(0));
                                                        }
                                                    }
                                                }
                                            }
                                            // Switch focus back to editor
                                            app.focus = Focus::Editor;
                                        }
                                    }
                                    KeyCode::Char('l') | KeyCode::Right => {
                                        app.focus = Focus::Editor;
                                    }
                                    KeyCode::Char('r') => {
                                        if let Some(i) = app.file_list_state.selected() {
                                            if i > 0 { // Can't rename "New Notebook"
                                                if let Some(path) = app.available_files.get(i - 1) {
                                                    app.rename_input = path.file_name().unwrap().to_string_lossy().to_string();
                                                    app.input_mode = InputMode::Renaming;
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('y') => {
                                        if let Some(i) = app.file_list_state.selected() {
                                            if i > 0 {
                                                if let Some(path) = app.available_files.get(i - 1) {
                                                    app.clipboard_file = Some(path.clone());
                                                    app.status_message = Some(format!("Yanked {}", path.display()));
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('p') => {
                                        if let Some(src) = &app.clipboard_file {
                                            if src.exists() {
                                                let mut dest = src.clone();
                                                let stem = src.file_stem().unwrap().to_string_lossy();
                                                let ext = src.extension().unwrap_or_default().to_string_lossy();
                                                
                                                let mut counter = 1;
                                                loop {
                                                    let new_name = if ext.is_empty() {
                                                        format!("{}_copy{}", stem, counter)
                                                    } else {
                                                        format!("{}_copy{}.{}", stem, counter, ext)
                                                    };
                                                    dest.set_file_name(new_name);
                                                    if !dest.exists() {
                                                        break;
                                                    }
                                                    counter += 1;
                                                }
                                                
                                                if fs::copy(src, &dest).is_ok() {
                                                    app.refresh_file_list();
                                                    app.status_message = Some(format!("Pasted to {}", dest.display()));
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char(':') => {
                                        app.input_mode = InputMode::Command;
                                        app.command_input.clear();
                                    }
                                    _ => {}
                                }
                            }
                            Focus::Editor => {
                                match key.code {
                                    KeyCode::Char('y') => {
                                        if let Some(i) = app.list_state.selected() {
                                            let cell_idx = i / 2;
                                            if let Some(cell) = app.cells.get(cell_idx) {
                                                if i % 2 == 0 {
                                                    app.clipboard_cell = Some(cell.clone());
                                                    app.status_message = Some("Cell yanked".to_string());
                                                } else if !cell.output.is_empty() {
                                                    if let Ok(mut clipboard) = Clipboard::new() {
                                                        if let Err(e) = clipboard.set_text(&cell.output) {
                                                            app.status_message = Some(format!("Clipboard error: {}", e));
                                                        } else {
                                                            app.status_message = Some("Output copied to clipboard".to_string());
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('p') => {
                                        if let Some(cell) = &app.clipboard_cell {
                                            if let Some(i) = app.list_state.selected() {
                                                let mut new_cell = cell.clone();
                                                new_cell.id = uuid::Uuid::new_v4().to_string();
                                                app.cells.insert((i / 2) + 1, new_cell);
                                                app.status_message = Some("Cell pasted".to_string());
                                            }
                                        }
                                    }
                                    KeyCode::Char('P') => {
                                        if let Some(cell) = &app.clipboard_cell {
                                            if let Some(i) = app.list_state.selected() {
                                                let mut new_cell = cell.clone();
                                                new_cell.id = uuid::Uuid::new_v4().to_string();
                                                app.cells.insert(i / 2, new_cell);
                                                app.status_message = Some("Cell pasted above".to_string());
                                            }
                                        }
                                    }
                                    KeyCode::Char(':') => {
                                        app.input_mode = InputMode::Command;
                                        app.command_input.clear();
                                    }
                                    KeyCode::Char('j') => {
                                        if let Some(i) = app.list_state.selected() {
                                            if i < app.cells.len() * 2 - 1 {
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
                                    KeyCode::Char('h') | KeyCode::Left => {
                                        if app.show_sidebar {
                                            app.focus = Focus::Sidebar;
                                        }
                                    }
                                    KeyCode::Char('o') => {
                                        if let Some(i) = app.list_state.selected() {
                                            app.insert_cell((i / 2) + 1, CellType::Shell);
                                        }
                                    }
                                    KeyCode::Char('O') => {
                                        if let Some(i) = app.list_state.selected() {
                                            app.insert_cell(i / 2, CellType::Shell);
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
                                    KeyCode::Char('r') => {
                                        app.input_mode = InputMode::Polling;
                                        app.polling_input.clear();
                                        app.polling_input.push('r');
                                    }
                                    KeyCode::Char('i') => {
                                        app.pending_delete = false;
                                        // Edit current cell
                                        if let Some(i) = app.list_state.selected() {
                                            let cell_idx = i / 2;
                                            if let Some(cell) = app.cells.get(cell_idx) {
                                                match cell.cell_type {
                                                    CellType::Shell => {
                                                        app.input = cell.content.clone();
                                                        app.input_mode = InputMode::Editing;
                                                    }
                                                    _ => {
                                                        // Open editor for all code cells
                                                        let content = cell.content.clone();
                                                        let ext = match cell.cell_type {
                                                            CellType::Rust => ".rs",
                                                            CellType::Python => ".py",
                                                            CellType::JavaScript => ".js",
                                                            CellType::TypeScript => ".ts",
                                                            CellType::C => ".c",
                                                            CellType::Cpp => ".cpp",
                                                            CellType::Go => ".go",
                                                            CellType::Shell => ".sh",
                                                        };
                                                        
                                                        // Suspend TUI
                                                        disable_raw_mode()?;
                                                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                                        
                                                        let new_content = open_editor(&content, ext)?;
                                                        
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
                                    }
                                    KeyCode::Enter => {
                                         app.pending_delete = false;
                                         // Run the selected cell or open output
                                         if let Some(i) = app.list_state.selected() {
                                            let cell_idx = i / 2;
                                            let is_output = i % 2 == 1;
                                            
                                            if is_output {
                                                // Try to open output file if present
                                                if let Some(cell) = app.cells.get(cell_idx) {
                                                    // Simple regex to find [Image: path]
                                                    // Or just look for the string
                                                    for line in cell.output.lines() {
                                                        if let Some(start) = line.find("[Image: ") {
                                                            if let Some(end) = line[start..].find(']') {
                                                                let path = &line[start + 8..start + end];
                                                                let _ = open::that(path);
                                                            }
                                                        }
                                                    }
                                                }
                                            } else {
                                                if let Some(req) = app.get_run_request(cell_idx) {
                                                    let client = client.clone();
                                                    let res = client.post("http://127.0.0.1:3000/exec")
                                                        .json(&req)
                                                        .send()
                                                        .await;

                                                    match res {
                                                        Ok(resp) => {
                                                            if let Ok(body) = resp.json::<CommandResponse>().await {
                                                                let mut output = format!("{}{}", body.stdout, body.stderr);
                                                                if let Some(display_data) = body.display_data {
                                                                    for data in display_data {
                                                                        if let Some(image_path) = data.data.get("image/png").or(data.data.get("image/svg+xml")) {
                                                                            if let Some(path_str) = image_path.as_str() {
                                                                                output.push_str(&format!("\n[Image: {}]", path_str));
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                app.update_cell_output(cell_idx, output);
                                                            } else {
                                                                app.update_cell_output(cell_idx, "Error parsing response".to_string());
                                                            }
                                                        }
                                                        Err(e) => {
                                                            app.update_cell_output(cell_idx, format!("Error connecting to server: {}", e));
                                                        }
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
                                }
                            }
                        }
                    }
                    InputMode::Command => match key.code {
                        KeyCode::Enter => {
                            match app.command_input.as_str() {
                                "q" => return Ok(()),
                                "w" => {
                                    app.save_notebook(None)?;
                                    app.input_mode = InputMode::Normal;
                                    app.refresh_file_list(); // Refresh list after save
                                }
                                "wq" => {
                                    app.save_notebook(None)?;
                                    return Ok(());
                                }
                                "export" => {
                                    app.input_mode = InputMode::Normal;
                                    if let Some(path) = &app.file_path {
                                        let notebook = Notebook { cells: app.cells.clone() };
                                        let client = client.clone();
                                        let res = client.post("http://127.0.0.1:3000/export")
                                            .json(&notebook)
                                            .send()
                                            .await;
                                        
                                        match res {
                                            Ok(resp) => {
                                                if let Ok(body) = resp.json::<ExportResponse>().await {
                                                    let mut export_path = path.clone();
                                                    export_path.set_extension("md");
                                                    if fs::write(&export_path, body.markdown).is_ok() {
                                                        app.status_message = Some(format!("Exported to {}", export_path.display()));
                                                    }
                                                }
                                            }
                                            Err(_) => {
                                                app.status_message = Some("Export failed".to_string());
                                            }
                                        }
                                    }
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
                                                        let mut output = format!("{}{}", body.stdout, body.stderr);
                                                        if let Some(display_data) = body.display_data {
                                                            for data in display_data {
                                                                if let Some(image_path) = data.data.get("image/png").or(data.data.get("image/svg+xml")) {
                                                                    if let Some(path_str) = image_path.as_str() {
                                                                        output.push_str(&format!("\n[Image: {}]", path_str));
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        app.update_cell_output(i, output);
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
                    InputMode::Polling => match key.code {
                        KeyCode::Char(c) => {
                            app.polling_input.push(c);
                        }
                        KeyCode::Enter => {
                            if let Some(i) = app.list_state.selected() {
                                let cell_idx = i / 2;
                                if let Some(cell) = app.cells.get_mut(cell_idx) {
                                    let input = &app.polling_input;
                                    if input == "r/" {
                                        cell.polling_interval = None;
                                        app.status_message = Some("Polling disabled".to_string());
                                    } else if input.starts_with("rm") {
                                        if let Ok(val) = input[2..].parse::<u64>() {
                                            cell.polling_interval = Some(val * 60);
                                            app.status_message = Some(format!("Polling set to {}s", val * 60));
                                        }
                                    } else if input.starts_with("rh") {
                                        if let Ok(val) = input[2..].parse::<u64>() {
                                            cell.polling_interval = Some(val * 3600);
                                            app.status_message = Some(format!("Polling set to {}s", val * 3600));
                                        }
                                    } else if input.starts_with('r') {
                                        if let Ok(val) = input[1..].parse::<u64>() {
                                            cell.polling_interval = Some(val);
                                            app.status_message = Some(format!("Polling set to {}s", val));
                                        }
                                    }
                                }
                            }
                            app.input_mode = InputMode::Normal;
                        }
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.polling_input.clear();
                        }
                        KeyCode::Backspace => {
                            app.polling_input.pop();
                        }
                        _ => {}
                    },
                    InputMode::Editing => match key.code {
                        KeyCode::Enter => {
                            let input = app.input.trim();
                            let (new_type, ext) = match input {
                                "rust" => (Some(CellType::Rust), ".rs"),
                                "py" | "python" => (Some(CellType::Python), ".py"),
                                "js" | "javascript" => (Some(CellType::JavaScript), ".js"),
                                "ts" | "typescript" => (Some(CellType::TypeScript), ".ts"),
                                "c" => (Some(CellType::C), ".c"),
                                "cpp" | "c++" => (Some(CellType::Cpp), ".cpp"),
                                "go" => (Some(CellType::Go), ".go"),
                                _ => (None, ""),
                            };

                            if let Some(cell_type) = new_type {
                                if let Some(cell) = app.current_cell_mut() {
                                    cell.cell_type = cell_type;
                                    cell.content = String::new(); // Clear input
                                    
                                    // Open editor
                                    disable_raw_mode()?;
                                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                    
                                    let new_content = open_editor(&cell.content, ext)?;
                                    
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
                                                    let mut output = format!("{}{}", body.stdout, body.stderr);
                                                    if let Some(display_data) = body.display_data {
                                                        for data in display_data {
                                                            if let Some(image_path) = data.data.get("image/png").or(data.data.get("image/svg+xml")) {
                                                                if let Some(path_str) = image_path.as_str() {
                                                                    output.push_str(&format!("\n[Image: {}]", path_str));
                                                                }
                                                            }
                                                        }
                                                    }
                                                    app.update_cell_output(i, output);
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
                    },
                    InputMode::Renaming => match key.code {
                        KeyCode::Enter => {
                            if let Some(i) = app.file_list_state.selected() {
                                if i > 0 {
                                    if let Some(old_path) = app.available_files.get(i - 1) {
                                        let mut new_path = old_path.clone();
                                        new_path.set_file_name(&app.rename_input);
                                        
                                        if fs::rename(old_path, &new_path).is_ok() {
                                            // Update file_path if we renamed the currently open file
                                            if let Some(current) = &app.file_path {
                                                if current == old_path {
                                                    app.file_path = Some(new_path);
                                                }
                                            }
                                            app.refresh_file_list();
                                            app.status_message = Some(format!("Renamed to {}", app.rename_input));
                                        } else {
                                            app.status_message = Some("Rename failed".to_string());
                                        }
                                    }
                                }
                            }
                            app.input_mode = InputMode::Normal;
                        }
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                        }
                        KeyCode::Char(c) => {
                            app.rename_input.push(c);
                        }
                        KeyCode::Backspace => {
                            app.rename_input.pop();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn open_editor(content: &str, extension: &str) -> io::Result<String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    
    let mut file = tempfile::Builder::new()
        .suffix(extension)
        .tempfile()?;
        
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
    let area = f.area();
    
    // 1. Split into Main Content (top) and Status/Command Bar (bottom)
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)].as_ref())
        .split(area);
    
    let main_content_area = root_chunks[0];
    let bottom_bar_area = root_chunks[1];

    // 2. Split Main Content into Sidebar and Editor
    let content_chunks = if app.show_sidebar {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(20), Constraint::Percentage(80)].as_ref())
            .split(main_content_area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, main_content_area)
    };

    // Render Sidebar
    if let Some(sidebar_area) = content_chunks.0 {
        let mut items = vec![ListItem::new("New Notebook").style(Style::default().add_modifier(Modifier::BOLD))];
        for path in &app.available_files {
            items.push(ListItem::new(path.file_name().unwrap().to_string_lossy()));
        }
        
        let border_style = if app.focus == Focus::Sidebar {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let list = List::new(items)
            .block(Block::default().borders(Borders::RIGHT).border_style(border_style))
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
            
        f.render_stateful_widget(list, sidebar_area, &mut app.file_list_state.clone());
    }

    // Render Editor
    let editor_area = content_chunks.1;
    
    let mut list_items = Vec::new();
    for (i, cell) in app.cells.iter().enumerate() {
        // Input Item
        let header = match cell.cell_type {
            CellType::Shell => "Shell",
            CellType::Rust => "Rust",
            CellType::Python => "Python",
            CellType::JavaScript => "JavaScript",
            CellType::TypeScript => "TypeScript",
            CellType::C => "C",
            CellType::Cpp => "C++",
            CellType::Go => "Go",
        };
        
        let countdown = if let Some(interval) = cell.polling_interval {
            if let Some(last) = cell.last_run {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                let next_run = last + interval;
                if next_run > now {
                    format!(" Executing in {}s...", next_run - now)
                } else {
                    " Executing...".to_string()
                }
            } else {
                " Executing...".to_string()
            }
        } else {
            "".to_string()
        };

        let content = if cell.content.is_empty() {
            "(empty)"
        } else {
            &cell.content
        };

        let style = if Some(i * 2) == app.list_state.selected() && app.focus == Focus::Editor {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let lines = vec![
            Line::from(Span::styled(format!("[{}] {}{}", header, cell.id, countdown), style)),
            Line::from(format!("In: {}", content)),
        ];
        list_items.push(ListItem::new(lines));

        // Output Item
        let output_style = if Some(i * 2 + 1) == app.list_state.selected() && app.focus == Focus::Editor {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let mut output_lines = vec![];
        if !cell.output.is_empty() {
            output_lines.push(Line::from(Span::styled("Out:", output_style)));
            for line in cell.output.lines() {
                output_lines.push(Line::from(format!("  {}", line)));
            }
        } else {
             output_lines.push(Line::from(Span::styled("Out: (empty)", output_style)));
        }
        output_lines.push(Line::from("")); // Spacer
        
        list_items.push(ListItem::new(output_lines));
    }

    let list = List::new(list_items)
        .block(Block::default().borders(Borders::NONE))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
        
    f.render_stateful_widget(list, editor_area, &mut app.list_state.clone());

    // Input box / Command bar
    match app.input_mode {
        InputMode::Editing => {
            if let Some(i) = app.list_state.selected() {
                if let Some(cell) = app.cells.get(i) {
                    if cell.cell_type == CellType::Shell {
                        let area = editor_area; // Use editor area for input positioning
                        let input_area = Rect::new(area.x, area.y + area.height.saturating_sub(3), area.width, 3);
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
            f.render_widget(input_block, bottom_bar_area);
        }
        InputMode::Polling => {
            let input_block = Paragraph::new(format!("{}", app.polling_input))
                .style(Style::default().fg(Color::Magenta));
            f.render_widget(input_block, bottom_bar_area);
        }
        InputMode::Normal => {
                let status = if let Some(msg) = &app.status_message {
                    msg.clone()
                } else if let Some(path) = &app.file_path {
                    path.file_name().unwrap().to_string_lossy().to_string()
                } else {
                    "[No Name]".to_string()
                };
                let status_block = Paragraph::new(status)
                .style(Style::default().fg(Color::DarkGray));
                f.render_widget(status_block, bottom_bar_area);
        }
        InputMode::Renaming => {
            let area = f.area();
            // Center popup
            let popup_area = Rect::new(area.width / 2 - 20, area.height / 2 - 1, 40, 3);
            f.render_widget(ratatui::widgets::Clear, popup_area);
            let input_block = Paragraph::new(app.rename_input.as_str())
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().borders(Borders::ALL).title("Rename File"));
            f.render_widget(input_block, popup_area);
        }
    }
}
