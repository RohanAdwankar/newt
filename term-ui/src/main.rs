use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, KeyboardEnhancementFlags, PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{error::Error, io, process::Command};
use tempfile::NamedTempFile;
use std::io::Write;

#[derive(Serialize)]
struct CommandRequest {
    command: String,
    language: Option<String>,
}

#[derive(Deserialize)]
struct CommandResponse {
    stdout: String,
    stderr: String,
    status: Option<i32>,
}

#[derive(Clone, PartialEq)]
enum CellType {
    Shell,
    Rust,
}

#[derive(Clone)]
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
}

enum InputMode {
    Normal,
    Editing,
}

impl App {
    fn new() -> App {
        let mut app = App {
            cells: Vec::new(),
            list_state: ListState::default(),
            input: String::new(),
            input_mode: InputMode::Editing, // Start in editing mode for the first cell
            pending_delete: false,
        };
        // Start with one empty shell cell
        app.add_cell(CellType::Shell);
        app
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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
    let mut app = App::new();

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
                match app.input_mode {
                    InputMode::Normal => match key.code {
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
                             if let Some(cell) = app.current_cell_mut() {
                                let cmd = cell.content.clone();
                                let lang = match cell.cell_type {
                                    CellType::Rust => Some("rust".to_string()),
                                    CellType::Shell => None,
                                };

                                let client = client.clone();
                                let res = client.post("http://127.0.0.1:3000/exec")
                                    .json(&CommandRequest { 
                                        command: cmd,
                                        language: lang,
                                    })
                                    .send()
                                    .await;

                                match res {
                                    Ok(resp) => {
                                        if let Ok(body) = resp.json::<CommandResponse>().await {
                                            cell.output = format!("{}{}", body.stdout, body.stderr);
                                        } else {
                                            cell.output = "Error parsing response".to_string();
                                        }
                                    }
                                    Err(e) => {
                                        cell.output = format!("Error connecting to server: {}", e);
                                    }
                                }
                             }
                        }
                        KeyCode::Esc => {
                            if app.pending_delete {
                                app.pending_delete = false;
                            } else {
                                return Ok(());
                            }
                        }
                        _ => {
                            app.pending_delete = false;
                        }
                    },
                    InputMode::Editing => match key.code {
                        KeyCode::Enter => {
                            // If typing "rust", switch to rust cell and open editor
                            if app.input.trim() == "rust" {
                                if let Some(cell) = app.current_cell_mut() {
                                    cell.cell_type = CellType::Rust;
                                    cell.content = String::new(); // Clear "rust"
                                    
                                    // Open editor
                                    // Suspend TUI
                                    disable_raw_mode()?;
                                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                    
                                    let new_content = open_editor(&cell.content)?;
                                    
                                    // Resume TUI
                                    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                                    enable_raw_mode()?;
                                    terminal.clear()?;

                                    cell.content = new_content;
                                    app.input_mode = InputMode::Normal; // Go to normal mode after editing
                                }
                            } else {
                                // Run the cell
                                let input_content = app.input.clone();
                                if let Some(cell) = app.current_cell_mut() {
                                    // Update content from input if it's a shell cell
                                    if cell.cell_type == CellType::Shell {
                                        cell.content = input_content;
                                    }
                                    
                                    let cmd = cell.content.clone();
                                    let lang = match cell.cell_type {
                                        CellType::Rust => Some("rust".to_string()),
                                        CellType::Shell => None,
                                    };

                                    let client = client.clone();
                                    let res = client.post("http://127.0.0.1:3000/exec")
                                        .json(&CommandRequest { 
                                            command: cmd,
                                            language: lang,
                                        })
                                        .send()
                                        .await;

                                    match res {
                                        Ok(resp) => {
                                            if let Ok(body) = resp.json::<CommandResponse>().await {
                                                cell.output = format!("{}{}", body.stdout, body.stderr);
                                            } else {
                                                cell.output = "Error parsing response".to_string();
                                            }
                                        }
                                        Err(e) => {
                                            cell.output = format!("Error connecting to server: {}", e);
                                        }
                                    }
                                }
                                
                                // After running, go to normal mode? Or create new cell?
                                // Let's create a new shell cell if we are at the bottom.
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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(1), Constraint::Length(3)].as_ref())
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

    // Input box (only visible/active when editing a shell cell)
    if let InputMode::Editing = app.input_mode {
        if let Some(i) = app.list_state.selected() {
            if let Some(cell) = app.cells.get(i) {
                if cell.cell_type == CellType::Shell {
                    let input_block = Paragraph::new(app.input.as_str())
                        .style(Style::default().fg(Color::Yellow))
                        .block(Block::default().borders(Borders::ALL).title("Input"));
                    f.render_widget(input_block, chunks[1]);
                }
            }
        }
    }
}
