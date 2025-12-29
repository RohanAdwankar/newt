use clap::Parser;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyboardEnhancementFlags, PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use directories::{ProjectDirs, UserDirs};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{error::Error, io, process::Command, path::PathBuf, fs};
use std::io::Write;

pub mod server;
pub mod markdown;

#[derive(Clone, Debug, PartialEq)]
pub struct FileItem {
    pub path: Option<PathBuf>,
    pub label: String,
    pub is_header: bool,
    pub is_app_file: bool,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Open a specific notebook file or the file menu
    #[arg(short, long, num_args=0..=1, default_missing_value = "")]
    open: Option<String>,

    /// Run in server mode (no TUI)
    #[arg(long)]
    serve: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommandRequest {
    pub command: String,
    pub language: Option<String>,
    #[serde(default, deserialize_with = "deserialize_context")]
    pub context: Option<Vec<String>>,
    #[serde(default)]
    pub client_type: Option<String>,
}

fn deserialize_context<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        String(String),
        Vec(Vec<String>),
    }

    let opt = Option::<StringOrVec>::deserialize(deserializer)?;
    match opt {
        Some(StringOrVec::String(s)) => {
            if s.is_empty() {
                Ok(None)
            } else {
                Ok(Some(vec![s]))
            }
        },
        Some(StringOrVec::Vec(v)) => Ok(Some(v)),
        None => Ok(None),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Notebook {
    pub cells: Vec<Cell>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExportResponse {
    pub markdown: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommandResponse {
    pub stdout: String,
    pub stderr: String,
    pub status: Option<i32>,
    pub display_data: Option<Vec<DisplayData>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DisplayData {
    pub data: std::collections::HashMap<String, serde_json::Value>,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum CellType {
    Shell,
    Rust,
    Python,
    JavaScript,
    TypeScript,
    C,
    Cpp,
    Go,
    Markdown,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
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
    pub available_files: Vec<FileItem>,
    pub pending_key: Option<char>,
    pub show_sidebar: bool,
    pub focus: Focus,
    pub clipboard_cell: Option<Cell>,
    pub clipboard_file: Option<PathBuf>,
    pub status_message: Option<String>,
    pub rename_input: String,
    pub polling_input: String,
    pub file_to_delete: Option<PathBuf>,
    pub running_cells: std::collections::HashSet<usize>,
    pub popup_input: String,
    pub popup_prompt: String,
    pub overwrite_path: Option<PathBuf>,
    pub editor: String,
    pub accent_color: Color,
    pub dirty: bool,
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
    ConfirmDelete,
    ConfirmOverwrite,
    InputPopup,
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
            file_to_delete: None,
            running_cells: std::collections::HashSet::new(),
            popup_input: String::new(),
            popup_prompt: String::new(),
            overwrite_path: None,
            editor: std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string()),
            accent_color: Color::Indexed(40),
            dirty: false,
        };

        app.load_config();

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
                            
                            let cells = crate::markdown::parse_markdown(&content);
                            if !cells.is_empty() {
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
            None => {
                // If no file specified, check for existing notebook in current directory
                for item in &app.available_files {
                    if !item.is_header && !item.is_app_file {
                        if let Some(path) = &item.path {
                            let ext = path.extension().unwrap_or_default().to_string_lossy();
                            if ext == "md" || ext == "newt" {
                                if let Ok(content) = fs::read_to_string(path) {
                                    let cells = if ext == "newt" {
                                        serde_json::from_str(&content).ok()
                                    } else {
                                        let c = crate::markdown::parse_markdown(&content);
                                        if c.is_empty() { None } else { Some(c) }
                                    };

                                    if let Some(c) = cells {
                                        app.cells = c;
                                        app.file_path = Some(path.clone());
                                        app.input_mode = InputMode::Normal;
                                        app.list_state.select(Some(0));
                                        return app;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Default to new notebook if no cells loaded
        if app.cells.is_empty() {
            app.add_cell(CellType::Shell);
        }
        
        app.dirty = false;
        
        app
    }

    fn refresh_file_list(&mut self) {
        self.available_files.clear();

        // Local Files
        self.available_files.push(FileItem {
            path: None,
            label: "Current Directory".to_string(),
            is_header: true,
            is_app_file: false,
        });

        if let Ok(entries) = fs::read_dir(".") {
            let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            files.sort();
            for path in files {
                // Ignore hidden files/dirs starting with .
                if let Some(name) = path.file_name() {
                    if name.to_string_lossy().starts_with('.') { continue; }
                }
                
                let label = path.file_name().unwrap().to_string_lossy().to_string();
                self.available_files.push(FileItem {
                    path: Some(path),
                    label,
                    is_header: false,
                    is_app_file: false,
                });
            }
        }

        // App Files
        self.available_files.push(FileItem {
            path: None,
            label: "Application Files".to_string(),
            is_header: true,
            is_app_file: true,
        });
        
        let dir = get_app_dir();
        if let Ok(entries) = fs::read_dir(dir) {
            let mut files: Vec<PathBuf> = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "newt" || ext == "md") {
                    files.push(path);
                }
            }
            files.sort();
            for path in files {
                let label = path.file_name().unwrap().to_string_lossy().to_string();
                self.available_files.push(FileItem {
                    path: Some(path),
                    label,
                    is_header: false,
                    is_app_file: true,
                });
            }
        }
        
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
        self.list_state.select(Some(index));
        self.input.clear();
        self.input_mode = InputMode::Normal;
        self.dirty = true;
    }

    fn delete_current_cell(&mut self) {
        if let Some(i) = self.list_state.selected() {
            let cell_idx = i;
            if self.cells.len() > 0 {
                self.cells.remove(cell_idx);
                self.dirty = true;
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
                    self.list_state.select(Some(new_idx));
                }
            }
        }
    }

    fn current_cell_mut(&mut self) -> Option<&mut Cell> {
        if let Some(i) = self.list_state.selected() {
            let cell_idx = i;
            self.cells.get_mut(cell_idx)
        } else {
            None
        }
    }

    fn save_notebook(&mut self, filename: Option<&str>) -> io::Result<()> {
        let path = if let Some(name) = filename {
             let p = PathBuf::from(name);
             if p.is_absolute() {
                 p
             } else {
                 let cwd = std::env::current_dir()?;
                 cwd.join(name)
             }
        } else if let Some(ref p) = self.file_path {
             p.clone()
        } else {
             let mut p = std::env::current_dir()?;
             p.push("notebook.md");
             
             // Check if file exists and increment name
             let mut counter = 2;
             while p.exists() {
                 p.set_file_name(format!("notebook{}.md", counter));
                 counter += 1;
             }
             p
        };
        
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let is_markdown = path.extension().map_or(false, |ext| ext == "md");
        let content = if is_markdown {
            crate::markdown::to_markdown(&self.cells)
        } else {
            serde_json::to_string_pretty(&self.cells)?
        };

        fs::write(&path, content)?;
        self.file_path = Some(path);
        self.dirty = false;
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
                CellType::Markdown => return None,
            };

            let mut context = Vec::new();
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
                             CellType::Markdown => None,
                        };
                        if prev_lang.as_ref() == Some(l) {
                            context.push(prev_cell.content.clone());
                        }
                    }
                }
            }
            
            let context_opt = if context.is_empty() { None } else { Some(context) };

            Some(CommandRequest { command: cmd, language: lang, context: context_opt, client_type: Some("tui".to_string()) })
        } else {
            None
        }
    }
    
    fn update_cell_output(&mut self, index: usize, output: String) {
        if let Some(cell) = self.cells.get_mut(index) {
            cell.output = output;
        }
    }

    fn load_config(&mut self) {
        let mut path = get_app_dir();
        path.push("config.json");
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<server::Config>(&content) {
                if let Some(editor) = config.editor {
                    self.editor = editor;
                }
                if let Some(color_index) = config.accent_color {
                    self.accent_color = Color::Indexed(color_index);
                }
            }
        }
    }

    fn save_config(&self) {
        let mut path = get_app_dir();
        path.push("config.json");
        
        let mut config = if let Ok(content) = fs::read_to_string(&path) {
            serde_json::from_str::<server::Config>(&content).unwrap_or_default()
        } else {
            server::Config::default()
        };
        
        config.editor = Some(self.editor.clone());
        if let Color::Indexed(i) = self.accent_color {
            config.accent_color = Some(i);
        }
        
        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = fs::write(path, json);
        }
    }

    fn handle_editor_result(&mut self, cell_idx: usize, new_content: String) {
        if let Some(cell) = self.cells.get_mut(cell_idx) {
            if cell.content != new_content {
                // Check if new content contains cell splitting fences
                if new_content.contains("```") {
                    let wrapped_content = if cell.cell_type == CellType::Markdown {
                        new_content.clone()
                    } else {
                        let lang = match cell.cell_type {
                            CellType::Rust => "rust",
                            CellType::Python => "python",
                            CellType::JavaScript => "javascript",
                            CellType::TypeScript => "typescript",
                            CellType::C => "c",
                            CellType::Cpp => "cpp",
                            CellType::Go => "go",
                            CellType::Shell => "bash",
                            CellType::Markdown => "markdown",
                        };
                        format!("```{}\n{}\n```", lang, new_content)
                    };

                    let new_cells = crate::markdown::parse_markdown(&wrapped_content);
                    if !new_cells.is_empty() {
                         // Replace current cell with new cells
                         self.cells.remove(cell_idx);
                         for (i, new_cell) in new_cells.into_iter().enumerate() {
                             self.cells.insert(cell_idx + i, new_cell);
                         }
                         self.dirty = true;
                         return;
                    }
                }

                if let Some(cell) = self.cells.get_mut(cell_idx) {
                    cell.content = new_content;
                    self.dirty = true;
                }
            }
        }
    }

    pub fn get_visual_items(&self) -> Vec<usize> {
        (0..self.cells.len()).collect()
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

    if args.serve {
        println!("Starting server on http://127.0.0.1:3030");
        server::run_server().await;
        return Ok(());
    }

    // Check if server is running
    let client = reqwest::Client::new();
    let server_running = client.get("http://127.0.0.1:3030").send().await.is_ok();
    
    if !server_running {
        // Start server in background
        tokio::spawn(async {
            server::run_server().await;
        });
        
        // Wait for server to start
        let mut retries = 0;
        while retries < 50 {
            if client.get("http://127.0.0.1:3030").send().await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            retries += 1;
        }
    }

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
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(usize, String)>(32);

    loop {
        // Check polling cells
        let indices_to_run = app.check_polling();

        for i in indices_to_run {
            if let Some(req) = app.get_run_request(i) {
                let client = client.clone();
                let tx = tx.clone();
                app.running_cells.insert(i);

                tokio::spawn(async move {
                    let res = client.post("http://127.0.0.1:3030/exec")
                        .json(&req)
                        .send()
                        .await;

                    let output = match res {
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
                                output
                            } else {
                                "Error parsing response".to_string()
                            }
                        }
                        Err(e) => format!("Error connecting to server: {}", e),
                    };
                    let _ = tx.send((i, output)).await;
                });
            }
        }

        // Check for finished tasks
        while let Ok((i, output)) = rx.try_recv() {
            app.running_cells.remove(&i);
            app.update_cell_output(i, output);
        }

        // Check for input requests
        if app.input_mode != InputMode::InputPopup {
            let _client = client.clone();
            // We can't await here easily in the sync loop, so we spawn a check
            // But we need to get the result back to the main thread.
            // Let's use another channel for input requests?
            // Or just do a blocking check since it's local fs?
            // The server writes to a file in temp dir.
            let temp_dir = std::env::temp_dir();
            let req_path = temp_dir.join("newt_web_input_req");
            let res_path = temp_dir.join("newt_web_input_res");
            
            if req_path.exists() && !res_path.exists() {
                if let Ok(prompt) = std::fs::read_to_string(&req_path) {
                    app.input_mode = InputMode::InputPopup;
                    app.popup_prompt = prompt;
                    app.popup_input.clear();
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
                                                app.focus = Focus::Editor;
                                            } else {
                                                // Open selected file
                                                if let Some(item) = app.available_files.get(i - 1) {
                                                    if item.is_header {
                                                        // Do nothing
                                                    } else if let Some(path) = &item.path {
                                                        let ext = path.extension().unwrap_or_default().to_string_lossy();
                                                        let is_notebook = ext == "newt" || ext == "md";
                                                        
                                                        let mut loaded = false;
                                                        if is_notebook {
                                                            if let Ok(content) = fs::read_to_string(path) {
                                                                if let Ok(cells) = serde_json::from_str(&content) {
                                                                    app.cells = cells;
                                                                    loaded = true;
                                                                } else {
                                                                    let cells = crate::markdown::parse_markdown(&content);
                                                                    if !cells.is_empty() {
                                                                        app.cells = cells;
                                                                        loaded = true;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        
                                                        if loaded {
                                                            app.file_path = Some(path.clone());
                                                            app.input_mode = InputMode::Normal;
                                                            app.list_state.select(Some(0));
                                                            app.focus = Focus::Editor;
                                                        } else {
                                                            // Open in external editor
                                                            disable_raw_mode()?;
                                                            execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                                                            
                                                            let _ = run_external_editor(path, &app.editor);
                                                            
                                                            execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
                                                            enable_raw_mode()?;
                                                            terminal.clear()?;
                                                        }
                                                    }
                                                }
                                            }
                                            // Switch focus back to editor
                                            if app.file_path.is_some() || app.cells.len() > 0 {
                                                app.focus = Focus::Editor;
                                            }
                                        }
                                    }
                                    KeyCode::Char('l') | KeyCode::Right => {
                                        app.focus = Focus::Editor;
                                    }
                                    KeyCode::Char('r') => {
                                        if let Some(i) = app.file_list_state.selected() {
                                            if i > 0 { 
                                                if let Some(item) = app.available_files.get(i - 1) {
                                                    if !item.is_header {
                                                        if let Some(path) = &item.path {
                                                            app.rename_input = path.file_name().unwrap().to_string_lossy().to_string();
                                                            app.input_mode = InputMode::Renaming;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('d') => {
                                        if let Some(i) = app.file_list_state.selected() {
                                            if i > 0 {
                                                if let Some(item) = app.available_files.get(i - 1) {
                                                    if !item.is_header {
                                                        if let Some(path) = &item.path {
                                                            app.file_to_delete = Some(path.clone());
                                                            app.input_mode = InputMode::ConfirmDelete;
                                                            app.command_input = format!("Remove {}? y/N: ", path.display());
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('y') => {
                                        if let Some(i) = app.file_list_state.selected() {
                                            if i > 0 {
                                                if let Some(item) = app.available_files.get(i - 1) {
                                                    if !item.is_header {
                                                        if let Some(path) = &item.path {
                                                            app.clipboard_file = Some(path.clone());
                                                            app.status_message = Some(format!("Yanked {}", path.display()));
                                                        }
                                                    }
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
                                let visual_items = app.get_visual_items();
                                match key.code {
                                    KeyCode::Char('y') => {
                                        if let Some(i) = app.list_state.selected() {
                                            if let Some(&cell_idx) = visual_items.get(i) {
                                                if let Some(cell) = app.cells.get(cell_idx) {
                                                    app.clipboard_cell = Some(cell.clone());
                                                    app.status_message = Some("Cell yanked".to_string());
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('p') => {
                                        if let Some(cell) = &app.clipboard_cell {
                                            if let Some(i) = app.list_state.selected() {
                                                if let Some(&cell_idx) = visual_items.get(i) {
                                                    let mut new_cell = cell.clone();
                                                    new_cell.id = uuid::Uuid::new_v4().to_string();
                                                    let idx = if app.cells.is_empty() { 0 } else { cell_idx + 1 };
                                                    app.cells.insert(idx, new_cell);
                                                    app.status_message = Some("Cell pasted".to_string());
                                                    app.dirty = true;
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('P') => {
                                        if let Some(cell) = &app.clipboard_cell {
                                            if let Some(i) = app.list_state.selected() {
                                                if let Some(&cell_idx) = visual_items.get(i) {
                                                    let mut new_cell = cell.clone();
                                                    new_cell.id = uuid::Uuid::new_v4().to_string();
                                                    app.cells.insert(cell_idx, new_cell);
                                                    app.status_message = Some("Cell pasted above".to_string());
                                                    app.dirty = true;
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char(':') => {
                                        app.input_mode = InputMode::Command;
                                        app.command_input.clear();
                                    }
                                    KeyCode::Char('j') => {
                                        if let Some(i) = app.list_state.selected() {
                                            if i < visual_items.len().saturating_sub(1) {
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
                                            if let Some(&cell_idx) = visual_items.get(i) {
                                                let idx = if app.cells.is_empty() { 0 } else { cell_idx + 1 };
                                                let type_to_add = if let Some(cell) = app.cells.get(cell_idx) {
                                                    cell.cell_type.clone()
                                                } else {
                                                    CellType::Shell
                                                };
                                                app.insert_cell(idx, type_to_add);
                                            }
                                        }
                                    }
                                    KeyCode::Char('O') => {
                                        if let Some(i) = app.list_state.selected() {
                                            if let Some(&cell_idx) = visual_items.get(i) {
                                                let type_to_add = if let Some(cell) = app.cells.get(cell_idx) {
                                                    cell.cell_type.clone()
                                                } else {
                                                    CellType::Shell
                                                };
                                                app.insert_cell(cell_idx, type_to_add);
                                            }
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
                                    KeyCode::Char('f') => {
                                        app.pending_delete = false;
                                        if let Some(i) = app.list_state.selected() {
                                            if let Some(&cell_idx) = visual_items.get(i) {
                                                let (cell_type, content) = if let Some(cell) = app.cells.get(cell_idx) {
                                                    (cell.cell_type.clone(), cell.content.clone())
                                                } else {
                                                    continue;
                                                };

                                                let ext = match cell_type {
                                                    CellType::Rust => ".rs",
                                                    CellType::Python => ".py",
                                                    CellType::JavaScript => ".js",
                                                    CellType::TypeScript => ".ts",
                                                    CellType::C => ".c",
                                                    CellType::Cpp => ".cpp",
                                                    CellType::Go => ".go",
                                                    CellType::Shell => ".sh",
                                                    CellType::Markdown => ".md",
                                                };

                                                // Suspend TUI
                                                let mut editor_cmd = app.editor.clone();
                                                let is_code = editor_cmd.trim().starts_with("code");
                                                if is_code && !editor_cmd.contains("--wait") && !editor_cmd.contains("-w") {
                                                    editor_cmd.push_str(" --wait");
                                                }

                                                if !is_code {
                                                    disable_raw_mode()?;
                                                    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                                                } else {
                                                    terminal.draw(|f| {
                                                        let area = f.area();
                                                        let popup_area = Rect::new(
                                                            area.width / 2 - 25,
                                                            area.height / 2 - 2,
                                                            50,
                                                            5,
                                                        );
                                                        f.render_widget(ratatui::widgets::Clear, popup_area);
                                                        let block = Block::default().borders(Borders::ALL).title("External Editor");
                                                        let text = Paragraph::new("Waiting for external editor...\nSave and close the file to return.\nOr press Enter to force return.")
                                                            .block(block)
                                                            .alignment(ratatui::layout::Alignment::Center);
                                                        f.render_widget(text, popup_area);
                                                    })?;
                                                }
                                                
                                                let res = open_editor(&content, ext, &editor_cmd, is_code);
                                                
                                                // Resume TUI
                                                if !is_code {
                                                    execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
                                                    enable_raw_mode()?;
                                                }
                                                terminal.clear()?; // Force redraw

                                                match res {
                                                    Ok(new_content) => {
                                                        app.handle_editor_result(cell_idx, new_content);
                                                    }
                                                    Err(e) => {
                                                        app.status_message = Some(format!("Editor error: {}", e));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('i') => {
                                        app.pending_delete = false;
                                        // Edit current cell
                                        if let Some(i) = app.list_state.selected() {
                                            if let Some(&cell_idx) = visual_items.get(i) {
                                                // Clone needed data to avoid holding borrow
                                                let (cell_type, content) = if let Some(cell) = app.cells.get(cell_idx) {
                                                    (cell.cell_type.clone(), cell.content.clone())
                                                } else {
                                                    continue;
                                                };

                                                match cell_type {
                                                    CellType::Shell => {
                                                        app.input = content;
                                                        app.input_mode = InputMode::Editing;
                                                    }
                                                    _ => {
                                                        // Open editor for all code cells
                                                        let ext = match cell_type {
                                                            CellType::Rust => ".rs",
                                                            CellType::Python => ".py",
                                                            CellType::JavaScript => ".js",
                                                            CellType::TypeScript => ".ts",
                                                            CellType::C => ".c",
                                                            CellType::Cpp => ".cpp",
                                                            CellType::Go => ".go",
                                                            CellType::Shell => ".sh",
                                                            CellType::Markdown => ".md",
                                                        };
                                                        
                                                        // Suspend TUI
                                                        let mut editor_cmd = app.editor.clone();
                                                        let is_code = editor_cmd.trim().starts_with("code");
                                                        if is_code && !editor_cmd.contains("--wait") && !editor_cmd.contains("-w") {
                                                            editor_cmd.push_str(" --wait");
                                                        }

                                                        if !is_code {
                                                            disable_raw_mode()?;
                                                            execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                                                        } else {
                                                            terminal.draw(|f| {
                                                                let area = f.area();
                                                                let popup_area = Rect::new(
                                                                    area.width / 2 - 25,
                                                                    area.height / 2 - 2,
                                                                    50,
                                                                    5,
                                                                );
                                                                f.render_widget(ratatui::widgets::Clear, popup_area);
                                                                let block = Block::default().borders(Borders::ALL).title("External Editor");
                                                                let text = Paragraph::new("Waiting for external editor...\nSave and close the file to return.\nOr press Enter to force return.")
                                                                    .block(block)
                                                                    .alignment(ratatui::layout::Alignment::Center);
                                                                f.render_widget(text, popup_area);
                                                            })?;
                                                        }
                                                        
                                                        let res = open_editor(&content, ext, &editor_cmd, is_code);
                                                        
                                                        // Resume TUI
                                                        if !is_code {
                                                            execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
                                                            enable_raw_mode()?;
                                                        }
                                                        terminal.clear()?; // Force redraw

                                                        match res {
                                                            Ok(new_content) => {
                                                                app.handle_editor_result(cell_idx, new_content);
                                                            }
                                                            Err(e) => {
                                                                app.status_message = Some(format!("Editor error: {}", e));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Enter => {
                                         app.pending_delete = false;
                                         if let Some(i) = app.list_state.selected() {
                                            let cell_idx = i;
                                            
                                            // Skip execution for Markdown
                                            if let Some(cell) = app.cells.get(cell_idx) {
                                                if cell.cell_type == CellType::Markdown {
                                                    return Ok(());
                                                }
                                            }

                                            let mut is_interactive = false;
                                            let mut cmd_to_run = String::new();
                                            let mut force_interactive = false;

                                            if let Some(cell) = app.cells.get(cell_idx) {
                                                if app.running_cells.contains(&cell_idx) {
                                                    force_interactive = true;
                                                    is_interactive = true;
                                                    if cell.cell_type == CellType::Python {
                                                        // Write to temp file
                                                        if let Ok(mut file) = tempfile::Builder::new().suffix(".py").tempfile() {
                                                            if write!(file, "{}", cell.content).is_ok() {
                                                                // Keep file alive by persisting it
                                                                let (_, path) = file.keep().unwrap();
                                                                cmd_to_run = format!("python3 {}", path.to_string_lossy());
                                                            }
                                                        }
                                                    } else {
                                                        cmd_to_run = cell.content.clone();
                                                    }
                                                } else if cell.cell_type == CellType::Shell {
                                                    let cmd = cell.content.trim();
                                                    if cmd.starts_with("vi") || cmd.starts_with("vim") || cmd.starts_with("nano") {
                                                        is_interactive = true;
                                                        cmd_to_run = cell.content.clone();
                                                    }
                                                }
                                            }

                                            if is_interactive {
                                                if force_interactive {
                                                    app.running_cells.remove(&cell_idx);
                                                    let _ = spawn_external_terminal(&cmd_to_run);
                                                } else {
                                                    disable_raw_mode()?;
                                                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                                    
                                                    let _ = run_interactive(&cmd_to_run);
                                                    
                                                    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                                                    enable_raw_mode()?;
                                                    terminal.clear()?;
                                                }
                                            } else if let Some(req) = app.get_run_request(cell_idx) {
                                                // Ensure the current cell is visible by snapping list offset
                                                if let Some(i) = app.list_state.selected() {
                                                    *app.list_state.offset_mut() = i;
                                                }
        
                                                let client = client.clone();
                                                let tx = tx.clone();
                                                app.running_cells.insert(cell_idx);
        
                                                tokio::spawn(async move {
                                                    let res = client.post("http://127.0.0.1:3030/exec")
                                                        .json(&req)
                                                        .send()
                                                        .await;

                                                    let output = match res {
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
                                                                output
                                                            } else {
                                                                "Error parsing response".to_string()
                                                            }
                                                        }
                                                        Err(e) => format!("Error connecting to server: {}", e),
                                                    };
                                                    let _ = tx.send((cell_idx, output)).await;
                                                });
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
                    InputMode::ConfirmOverwrite => match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            if let Some(path) = app.overwrite_path.clone() {
                                let path_str = path.to_string_lossy().to_string();
                                app.save_notebook(Some(&path_str))?;
                                app.status_message = Some(format!("Saved to {}", path.display()));
                                app.refresh_file_list();
                            }
                            app.input_mode = InputMode::Normal;
                            app.command_input.clear();
                            app.overwrite_path = None;
                        }
                        _ => {
                            app.status_message = Some("Save cancelled".to_string());
                            app.input_mode = InputMode::Normal;
                            app.command_input.clear();
                            app.overwrite_path = None;
                        }
                    },
                    InputMode::Command => match key.code {
                        KeyCode::Enter => {
                            let cmd = app.command_input.clone();
                            if cmd.starts_with("w ") {
                                let mut filename = cmd[2..].trim().to_string();
                                if !filename.is_empty() {
                                    if !filename.ends_with(".newt") {
                                        filename.push_str(".newt");
                                    }
                                    let p = PathBuf::from(&filename);
                                    let path = if p.is_absolute() {
                                        p
                                    } else {
                                        get_app_dir().join(&filename)
                                    };

                                    if path.exists() {
                                        app.overwrite_path = Some(path.clone());
                                        app.input_mode = InputMode::ConfirmOverwrite;
                                        app.command_input = format!("Overwrite {}? y/N: ", path.display());
                                    } else {
                                        // Pass the full path string to save_notebook
                                        let path_str = path.to_string_lossy().to_string();
                                        app.save_notebook(Some(&path_str))?;
                                        app.input_mode = InputMode::Normal;
                                        app.refresh_file_list();
                                        app.command_input.clear();
                                    }
                                } else {
                                    app.input_mode = InputMode::Normal;
                                    app.command_input.clear();
                                }
                            } else if cmd.starts_with("editor ") {
                                let editor = cmd[7..].trim().to_string();
                                if !editor.is_empty() {
                                    app.editor = editor.clone();
                                    app.save_config();
                                    app.status_message = Some(format!("Editor set to {}", editor));
                                }
                                app.input_mode = InputMode::Normal;
                                app.command_input.clear();
                            } else if cmd.starts_with("color ") {
                                let color_str = cmd[6..].trim();
                                if let Ok(color_idx) = color_str.parse::<u8>() {
                                    app.accent_color = Color::Indexed(color_idx);
                                    app.save_config();
                                    app.status_message = Some(format!("Accent color set to {}", color_idx));
                                } else {
                                    app.status_message = Some("Invalid color index".to_string());
                                }
                                app.input_mode = InputMode::Normal;
                                app.command_input.clear();
                            } else {
                                match app.command_input.as_str() {
                                "q" => {
                                    let is_untitled_empty = app.file_path.is_none() && app.cells.iter().all(|c| c.content.trim().is_empty());
                                    if app.dirty && !is_untitled_empty {
                                        app.status_message = Some("E37: No write since last change (add ! to override)".to_string());
                                        app.input_mode = InputMode::Normal;
                                    } else {
                                        return Ok(());
                                    }
                                }
                                "q!" => return Ok(()),
                                "w" => {
                                    app.save_notebook(None)?;
                                    app.input_mode = InputMode::Normal;
                                    app.refresh_file_list(); // Refresh list after save
                                }
                                "ww" => {
                                    let filename = if let Some(p) = &app.file_path {
                                        p.file_name().unwrap().to_string_lossy().to_string()
                                    } else {
                                        "notebook.md".to_string()
                                    };
                                    let mut path = get_app_dir();
                                    path.push(&filename);
                                    let path_str = path.to_string_lossy().to_string();
                                    
                                    app.save_notebook(Some(&path_str))?;
                                    app.status_message = Some(format!("Saved to {}", path.display()));
                                    app.input_mode = InputMode::Normal;
                                    app.refresh_file_list();
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
                                        let res = client.post("http://127.0.0.1:3030/export")
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
                                    let mut requests = Vec::new();
                                    for i in 0..app.cells.len() {
                                        if let Some(req) = app.get_run_request(i) {
                                            requests.push((i, req));
                                            app.running_cells.insert(i);
                                        }
                                    }

                                    let client = client.clone();
                                    let tx = tx.clone();

                                    tokio::spawn(async move {
                                        for (i, req) in requests {
                                            let res = client.post("http://127.0.0.1:3030/exec")
                                                .json(&req)
                                                .send()
                                                .await;

                                            let output = match res {
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
                                                        output
                                                    } else {
                                                        "Error parsing response".to_string()
                                                    }
                                                }
                                                Err(e) => format!("Error connecting to server: {}", e),
                                            };
                                            let _ = tx.send((i, output)).await;
                                        }
                                    });
                                }
                                "rust" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i / 2) {
                                            cell.cell_type = CellType::Rust;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "cpp" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i / 2) {
                                            cell.cell_type = CellType::Cpp;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "c" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i / 2) {
                                            cell.cell_type = CellType::C;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "py" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i / 2) {
                                            cell.cell_type = CellType::Python;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "ts" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i / 2) {
                                            cell.cell_type = CellType::TypeScript;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "js" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i / 2) {
                                            cell.cell_type = CellType::JavaScript;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "md" | "markdown" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i / 2) {
                                            cell.cell_type = CellType::Markdown;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "sh" | "shell" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i / 2) {
                                            cell.cell_type = CellType::Shell;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                _ => {
                                    app.input_mode = InputMode::Normal;
                                }
                            }
                            app.command_input.clear();
                            }
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
                                        app.dirty = true;
                                    } else if input.starts_with("rm") {
                                        if let Ok(val) = input[2..].parse::<u64>() {
                                            cell.polling_interval = Some(val * 60);
                                            app.status_message = Some(format!("Polling set to {}s", val * 60));
                                            app.dirty = true;
                                        }
                                    } else if input.starts_with("rh") {
                                        if let Ok(val) = input[2..].parse::<u64>() {
                                            cell.polling_interval = Some(val * 3600);
                                            app.status_message = Some(format!("Polling set to {}s", val * 3600));
                                            app.dirty = true;
                                        }
                                    } else if input.starts_with('r') {
                                        if let Ok(val) = input[1..].parse::<u64>() {
                                            cell.polling_interval = Some(val);
                                            app.status_message = Some(format!("Polling set to {}s", val));
                                            app.dirty = true;
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
                                "md" | "markdown" => (Some(CellType::Markdown), ".md"),
                                _ => (None, ""),
                            };

                            if let Some(cell_type) = new_type {
                                let mut editor_cmd = app.editor.clone();
                                let is_code = editor_cmd.trim().starts_with("code");
                                if is_code && !editor_cmd.contains("--wait") && !editor_cmd.contains("-w") {
                                    editor_cmd.push_str(" --wait");
                                }

                                if let Some(cell) = app.current_cell_mut() {
                                    cell.cell_type = cell_type;
                                    cell.content = String::new(); // Clear input
                                    
                                    // Open editor
                                    if !is_code {
                                        disable_raw_mode()?;
                                        execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                                    } else {
                                        terminal.draw(|f| {
                                            let area = f.area();
                                            let popup_area = Rect::new(
                                                area.width / 2 - 25,
                                                area.height / 2 - 2,
                                                50,
                                                5,
                                            );
                                            f.render_widget(ratatui::widgets::Clear, popup_area);
                                            let block = Block::default().borders(Borders::ALL).title("External Editor");
                                            let text = Paragraph::new("Waiting for external editor...\nSave and close the file to return.\nOr press Enter to force return.")
                                                .block(block)
                                                .alignment(ratatui::layout::Alignment::Center);
                                            f.render_widget(text, popup_area);
                                        })?;
                                    }
                                    
                                    let res = open_editor(&cell.content, ext, &editor_cmd, is_code);
                                    
                                    if !is_code {
                                        execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
                                        enable_raw_mode()?;
                                    }
                                    terminal.clear()?;

                                    match res {
                                        Ok(new_content) => {
                                            if new_content != cell.content {
                                                cell.content = new_content;
                                                app.dirty = true;
                                            }
                                            app.input_mode = InputMode::Normal; 
                                        }
                                        Err(e) => {
                                            app.status_message = Some(format!("Editor error: {}", e));
                                            app.input_mode = InputMode::Normal;
                                        }
                                    } 
                                }
                            } else {
                                // Run the cell
                                let input_content = app.input.clone();
                                let mut is_interactive = false;
                                let mut cmd_to_run = String::new();

                                if let Some(cell) = app.current_cell_mut() {
                                    if cell.cell_type == CellType::Shell {
                                        cell.content = input_content;
                                        let cmd = cell.content.trim();
                                        if cmd.starts_with("vi") || cmd.starts_with("vim") || cmd.starts_with("nano") {
                                            is_interactive = true;
                                            cmd_to_run = cell.content.clone();
                                        }
                                    }
                                }
                                
                                if is_interactive {
                                    disable_raw_mode()?;
                                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                    
                                    let _ = run_interactive(&cmd_to_run);
                                    
                                    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                                    enable_raw_mode()?;
                                    terminal.clear()?;
                                    app.input_mode = InputMode::Normal;
                                } else if let Some(i) = app.list_state.selected() {
                                    if let Some(req) = app.get_run_request(i) {
                                        let client = client.clone();
                                        let tx = tx.clone();
                                        app.running_cells.insert(i);

                                        tokio::spawn(async move {
                                            let res = client.post("http://127.0.0.1:3030/exec")
                                                .json(&req)
                                                .send()
                                                .await;

                                            let output = match res {
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
                                                        output
                                                    } else {
                                                        "Error parsing response".to_string()
                                                    }
                                                }
                                                Err(e) => format!("Error connecting to server: {}", e),
                                            };
                                            let _ = tx.send((i, output)).await;
                                        });
                                    }
                                }
                                
                                if let Some(i) = app.list_state.selected() {
                                    if i == app.cells.len() * 2 - 2 { // Last cell input
                                        let type_to_add = if let Some(cell) = app.cells.last() {
                                            cell.cell_type.clone()
                                        } else {
                                            CellType::Shell
                                        };
                                        app.add_cell(type_to_add);
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
                                    if let Some(item) = app.available_files.get(i - 1) {
                                        if let Some(old_path) = &item.path {
                                            let mut new_path = old_path.clone();
                                            let new_name = app.rename_input.clone();
                                            new_path.set_file_name(&new_name);
                                            
                                            if fs::rename(old_path, &new_path).is_ok() {
                                                // Update file_path if we renamed the currently open file
                                                if let Some(current) = &app.file_path {
                                                    if current == old_path {
                                                        app.file_path = Some(new_path);
                                                    }
                                                }
                                                app.refresh_file_list();
                                                app.status_message = Some(format!("Renamed to {}", new_name));
                                            } else {
                                                app.status_message = Some("Rename failed".to_string());
                                            }
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
                    },
                    InputMode::InputPopup => match key.code {
                        KeyCode::Enter => {
                            // Submit input directly to file to avoid race condition
                            let temp_dir = std::env::temp_dir();
                            let res_path = temp_dir.join("newt_web_input_res");
                            if let Ok(_) = std::fs::write(res_path, &app.popup_input) {
                                app.input_mode = InputMode::Normal;
                                app.popup_input.clear();
                            }
                        }
                        KeyCode::Char(c) => {
                            app.popup_input.push(c);
                        }
                        KeyCode::Backspace => {
                            app.popup_input.pop();
                        }
                        _ => {}
                    },
                    InputMode::ConfirmDelete => match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            if let Some(path) = &app.file_to_delete {
                                if let Err(e) = fs::remove_file(path) {
                                    app.status_message = Some(format!("Error deleting file: {}", e));
                                } else {
                                    app.status_message = Some(format!("Deleted {}", path.display()));
                                    app.refresh_file_list();
                                }
                            }
                            app.input_mode = InputMode::Normal;
                            app.command_input.clear();
                            app.file_to_delete = None;
                        }
                        _ => {
                            app.status_message = Some("Delete cancelled".to_string());
                            app.input_mode = InputMode::Normal;
                            app.command_input.clear();
                            app.file_to_delete = None;
                        }
                    }
                }
            }
        }
    }
    // Ok(()) // Unreachable
}

fn open_editor(content: &str, extension: &str, editor_cmd: &str, interactive_wait: bool) -> io::Result<String> {
    let mut file = tempfile::Builder::new()
        .suffix(extension)
        .tempfile()?;
        
    write!(file, "{}", content)?;
    file.flush()?;
    
    let path = file.path().to_str().unwrap().to_string();
    
    // Split command into program and args
    let parts: Vec<&str> = editor_cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Empty editor command"));
    }
    
    let mut program = parts[0].to_string();
    // Expand ~ to home directory
    if program.starts_with("~/") {
        if let Some(user_dirs) = UserDirs::new() {
            let home = user_dirs.home_dir().to_string_lossy();
            program = program.replace("~", &home);
        }
    }

    let args = &parts[1..];
    
    let mut child = Command::new(program)
        .args(args)
        .arg(&path)
        .spawn()?;
        
    if interactive_wait {
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if event::poll(std::time::Duration::from_millis(100))? {
                        if let Event::Key(key) = event::read()? {
                            if key.code == KeyCode::Enter {
                                // User forced return
                                let _ = child.kill();
                                break;
                            }
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }
    } else {
        let status = child.wait()?;
        if !status.success() {
            return Err(io::Error::new(io::ErrorKind::Other, "Editor exited with error"));
        }
    }
    
    let new_content = std::fs::read_to_string(&path)?;
    Ok(new_content)
}

fn run_external_editor(path: &std::path::Path, editor_cmd: &str) -> io::Result<()> {
    // Split command into program and args
    let parts: Vec<&str> = editor_cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Empty editor command"));
    }
    
    let mut program = parts[0].to_string();
    // Expand ~ to home directory
    if program.starts_with("~/") {
        if let Some(user_dirs) = UserDirs::new() {
            let home = user_dirs.home_dir().to_string_lossy();
            program = program.replace("~", &home);
        }
    }

    let args = &parts[1..];
    
    // Check if we need to wait
    let is_code = editor_cmd.trim().starts_with("code");
    
    // For terminal editors (vi, nano), we just spawn and wait.
    // For GUI editors (code), we need --wait to block.
    
    let mut cmd = Command::new(program);
    cmd.args(args);
    
    if is_code && !editor_cmd.contains("--wait") && !editor_cmd.contains("-w") {
        cmd.arg("--wait");
    }
    
    cmd.arg(path);
    
    let mut child = cmd.spawn()?;
    
    // Wait for it to finish
    let _ = child.wait()?;
    
    Ok(())
}

fn spawn_external_terminal(command: &str) -> io::Result<()> {
    let escaped_cmd = command.replace("\\", "\\\\").replace("\"", "\\\"");
    let applescript = format!("tell application \"Terminal\" to do script \"{}\"", escaped_cmd);
    
    Command::new("osascript")
        .arg("-e")
        .arg(applescript)
        .spawn()?;
        
    Ok(())
}

fn run_interactive(command: &str) -> io::Result<()> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(());
    }
    let program = parts[0];
    let args = &parts[1..];
    
    let status = Command::new(program)
        .args(args)
        .status()?;
        
    if !status.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "Command exited with error"));
    }
    Ok(())
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
        for item in &app.available_files {
            if item.is_header {
                items.push(ListItem::new(Span::styled(format!("--- {} ---", item.label), Style::default().fg(Color::DarkGray))));
            } else {
                let name = if let Some(path) = &item.path {
                    path.file_name().unwrap().to_string_lossy().to_string()
                } else {
                    item.label.clone()
                };
                items.push(ListItem::new(format!("  {}", name)));
            }
        }
        
        let border_style = if app.focus == Focus::Sidebar {
            Style::default().fg(app.accent_color)
        } else {
            Style::default()
        };

        let list = List::new(items)
            .block(Block::default().borders(Borders::RIGHT).border_style(border_style))
            .highlight_style(Style::default().fg(app.accent_color).add_modifier(Modifier::BOLD));
            
        f.render_stateful_widget(list, sidebar_area, &mut app.file_list_state.clone());
    }

    // Render Editor
    let editor_area = content_chunks.1;
    
    let mut list_items = Vec::new();
    let visual_items = app.get_visual_items();

    for &i in visual_items.iter() {
        let cell = &app.cells[i];
        let mut cell_lines = Vec::new();
        
        // Input Section
        let header = match cell.cell_type {
            CellType::Shell => "Shell",
            CellType::Rust => "Rust",
            CellType::Python => "Python",
            CellType::JavaScript => "JavaScript",
            CellType::TypeScript => "TypeScript",
            CellType::C => "C",
            CellType::Cpp => "C++",
            CellType::Go => "Go",
            CellType::Markdown => "Markdown",
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

        let is_selected = Some(i) == app.list_state.selected() && app.focus == Focus::Editor;
        let style = if is_selected {
            Style::default().fg(app.accent_color).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let header_str = format!("[{}]{}", header, countdown);
        let width = editor_area.width as usize;
        let padding_len = width.saturating_sub(3 + header_str.len()).saturating_sub(2);
        let padding = " ".repeat(padding_len);

        if cell.cell_type == CellType::Markdown {
             let is_editing_this = app.input_mode == InputMode::Editing && is_selected;
             
             if is_editing_this {
                 cell_lines.push(Line::from(vec![
                    Span::styled("In:", style),
                    Span::raw(padding.clone()),
                    Span::styled(header_str.clone(), style),
                ]));
                 if cell.content.is_empty() {
                     cell_lines.push(Line::from("     (empty)"));
                 } else {
                     for line in cell.content.lines() {
                         cell_lines.push(Line::from(format!("     {}", line)));
                     }
                 }
             } else {
                 let preview_padding_len = width.saturating_sub(header_str.len()).saturating_sub(2);
                 let preview_padding = " ".repeat(preview_padding_len);
                 cell_lines.push(Line::from(vec![
                    Span::raw(preview_padding),
                    Span::styled(header_str.clone(), style),
                ]));
                 if cell.content.is_empty() {
                     cell_lines.push(Line::from("(empty)"));
                 } else {
                     for line in cell.content.lines() {
                         if line.starts_with("# ") {
                             cell_lines.push(Line::from(Span::styled(line, Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD))));
                         } else if line.starts_with("## ") {
                             cell_lines.push(Line::from(Span::styled(line, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))));
                         } else if line.starts_with("### ") {
                             cell_lines.push(Line::from(Span::styled(line, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))));
                         } else if line.starts_with("- ") || line.starts_with("* ") {
                             cell_lines.push(Line::from(format!("  • {}", &line[2..])));
                         } else {
                             cell_lines.push(Line::from(line));
                         }
                     }
                 }
             }
        } else {
             cell_lines.push(Line::from(vec![
                Span::styled("In:", style),
                Span::raw(padding),
                Span::styled(header_str, style),
            ]));
             if cell.content.is_empty() {
                 cell_lines.push(Line::from("     (empty)"));
             } else {
                 for line in cell.content.lines() {
                     cell_lines.push(Line::from(format!("     {}", line)));
                 }
             }
        };
        
        // Output Section (combined)
        if cell.cell_type != CellType::Markdown {
            cell_lines.push(Line::from("")); // Spacer between input and output
            
            let output_style = if is_selected {
                Style::default().fg(app.accent_color).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            if !cell.output.is_empty() {
                cell_lines.push(Line::from(Span::styled("Out:", output_style)));
                let lines: Vec<&str> = cell.output.lines().collect();
                if lines.len() > 10 {
                    for line in lines.iter().take(10) {
                        cell_lines.push(Line::from(format!("     {}", line)));
                    }
                    cell_lines.push(Line::from("     ....."));
                } else {
                    for line in lines {
                        cell_lines.push(Line::from(format!("     {}", line)));
                    }
                }
            } else {
                 cell_lines.push(Line::from(Span::styled("Out:", output_style)));
                 cell_lines.push(Line::from("     (empty)"));
            }
        }
        
        cell_lines.push(Line::from("")); // Bottom spacer
        list_items.push(ListItem::new(cell_lines));
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
                        let area = editor_area; 
                        let input_area = Rect::new(area.x, area.y + area.height.saturating_sub(3), area.width, 3);
                        f.render_widget(ratatui::widgets::Clear, input_area);
                        
                        let input_block = Paragraph::new(app.input.as_str())
                            .style(Style::default().fg(app.accent_color))
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
        InputMode::ConfirmDelete | InputMode::ConfirmOverwrite => {
            let input_block = Paragraph::new(format!("{}", app.command_input))
                .style(Style::default().fg(Color::Red));
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
                .style(Style::default().fg(app.accent_color))
                .block(Block::default().borders(Borders::ALL).title("Rename File"));
            f.render_widget(input_block, popup_area);
        }
        InputMode::InputPopup => {
            let area = f.area();
            // Center popup
            let popup_area = Rect::new(area.width / 2 - 30, area.height / 2 - 2, 60, 5);
            f.render_widget(ratatui::widgets::Clear, popup_area);
            
            let block = Block::default()
                .borders(Borders::ALL)
                .title("Input Required")
                .style(Style::default().fg(Color::Cyan));
                
            let inner_area = block.inner(popup_area);
            
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)].as_ref())
                .split(inner_area);
                
            let prompt = Paragraph::new(app.popup_prompt.as_str());
            let input = Paragraph::new(app.popup_input.as_str()).style(Style::default().fg(app.accent_color));
            
            f.render_widget(block, popup_area);
            f.render_widget(prompt, chunks[0]);
            f.render_widget(input, chunks[2]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_request_deserialization() {
        // Case 1: Context is a list of strings (Normal case)
        let json = r#"{"command": "print(1)", "language": "python", "context": ["a=1"]}"#;
        let req: CommandRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.context, Some(vec!["a=1".to_string()]));

        // Case 2: Context is an empty string (The bug case)
        let json = r#"{"command": "print(1)", "language": "python", "context": ""}"#;
        let req: CommandRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.context, None);

        // Case 3: Context is a non-empty string (Backward compatibility)
        let json = r#"{"command": "print(1)", "language": "python", "context": "a=1"}"#;
        let req: CommandRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.context, Some(vec!["a=1".to_string()]));

        // Case 4: Context is null/missing
        let json = r#"{"command": "print(1)", "language": "python"}"#;
        let req: CommandRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.context, None);
    }
}
