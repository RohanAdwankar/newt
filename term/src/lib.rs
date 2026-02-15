use clap::Parser;
use arboard::Clipboard;

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
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;

pub mod server;
pub mod markdown;

lazy_static::lazy_static! {
    static ref SYNTAX_SET: SyntaxSet = SyntaxSet::load_defaults_newlines();
    static ref THEME_SET: ThemeSet = ThemeSet::load_defaults();
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileItem {
    pub path: Option<PathBuf>,
    pub label: String,
    pub is_header: bool,
    pub is_app_file: bool,
    pub is_directory: bool,
    pub is_expanded: bool,
    pub depth: usize,
    pub parent_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct SearchState {
    pub query: String,
    pub matches: Vec<usize>,
    pub current_match: Option<usize>,
    pub is_smart_case: bool,
}

impl SearchState {
    fn new() -> Self {
        SearchState {
            query: String::new(),
            matches: Vec::new(),
            current_match: None,
            is_smart_case: true,
        }
    }

    fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current_match = None;
    }

    fn has_uppercase(&self) -> bool {
        self.query.chars().any(|c| c.is_uppercase())
    }

    fn matches_pattern(&self, text: &str) -> bool {
        if self.is_smart_case && self.has_uppercase() {
            // Case sensitive
            text.contains(&self.query)
        } else {
            // Case insensitive
            text.to_lowercase().contains(&self.query.to_lowercase())
        }
    }
}

use clap::Subcommand;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to a file to open
    file_path: Option<String>,

    /// Open a specific notebook file or the file menu
    #[arg(short, long, num_args=0..=1, default_missing_value = "")]
    open: Option<String>,

    /// Run in server mode (no TUI)
    #[arg(long)]
    serve: bool,

    /// Convert terminal output to newt markdown
    #[arg(long)]
    term: Option<String>,

    /// Skip confirmation prompt when using --term
    #[arg(long)]
    quiet: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a markdown file (optionally specify a heading to run only those cells)
    Run {
        /// Path to the markdown file
        file: String,
        /// Optional heading to filter cells
        heading: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommandRequest {
    pub command: String,
    pub language: Option<String>,
    #[serde(default, deserialize_with = "deserialize_context")]
    pub context: Option<Vec<String>>,
    #[serde(default)]
    pub client_type: Option<String>,
    #[serde(default)]
    pub notebook_path: Option<String>,
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
    pub shell_cursor: usize,
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
    pub clipboard_output: Option<String>,
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
    pub display_mode: String,
    pub colorscheme: String,
    pub dirty: bool,
    pub last_file_refresh: std::time::Instant,
    pub numeric_prefix: Option<usize>,
    pub sidebar_search: SearchState,
    pub editor_search: SearchState,
    pub expanded_dirs: std::collections::HashSet<PathBuf>,
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
    SearchSidebar,
    SearchEditor,
}

impl App {
    pub fn new(open_arg: Option<String>) -> App {
        let mut app = App {
            cells: Vec::new(),
            list_state: ListState::default(),
            input: String::new(),
            shell_cursor: 0,
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
            clipboard_output: None,
            status_message: None,
            rename_input: String::new(),
            polling_input: String::new(),
            file_to_delete: None,
            running_cells: std::collections::HashSet::new(),
            popup_input: String::new(),
            popup_prompt: String::new(),
            overwrite_path: None,
            editor: std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string()),
            accent_color: Color::Indexed(183),  // Pastel purple/mauve (default for pastel colorscheme)
            display_mode: "compact".to_string(),
            colorscheme: "pastel".to_string(),
            dirty: false,
            last_file_refresh: std::time::Instant::now(),
            numeric_prefix: None,
            sidebar_search: SearchState::new(),
            editor_search: SearchState::new(),
            expanded_dirs: std::collections::HashSet::new(),
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
                                app.list_state.select(Some(0));
                                app.set_default_mode_for_selected_cell();
                                return app;
                            }
                            
                            let cells = crate::markdown::parse_markdown(&content);
                            if !cells.is_empty() {
                                app.cells = cells;
                                app.file_path = Some(path);
                                app.list_state.select(Some(0));
                                app.set_default_mode_for_selected_cell();
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
                                        app.list_state.select(Some(0));
                                        app.set_default_mode_for_selected_cell();
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
        // Preserve the current selection
        let current_selection = self.file_list_state.selected();

        self.available_files.clear();

        // Local Files Header
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let cwd_label = cwd
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| cwd.to_string_lossy().to_string());

        self.available_files.push(FileItem {
            path: None,
            label: cwd_label,
            is_header: true,
            is_app_file: false,
            is_directory: false,
            is_expanded: false,
            depth: 0,
            parent_path: None,
        });

        // Build hierarchical tree for local files
        self.build_directory_tree(&cwd, 0, false);

        // App Files Header
        self.available_files.push(FileItem {
            path: None,
            label: "Application Files".to_string(),
            is_header: true,
            is_app_file: true,
            is_directory: false,
            is_expanded: false,
            depth: 0,
            parent_path: None,
        });

        // Build hierarchical tree for app files
        let app_dir = get_app_dir();
        self.build_directory_tree(&app_dir, 0, true);

        // Restore the previous selection, or default to 0 if it was None
        // Also ensure the selection is within bounds
        if let Some(selected) = current_selection {
            let max_index = self.available_files.len().saturating_sub(1);
            self.file_list_state.select(Some(selected.min(max_index)));
        } else {
            self.file_list_state.select(Some(0));
        }
    }

    fn build_directory_tree(&mut self, dir: &PathBuf, depth: usize, is_app_file: bool) {
        if let Ok(entries) = fs::read_dir(dir) {
            let mut items: Vec<(PathBuf, bool)> = entries
                .flatten()
                .map(|e| {
                    let path = e.path();
                    let is_dir = path.is_dir();
                    (path, is_dir)
                })
                .collect();

            // Sort: directories first, then files, alphabetically within each group
            items.sort_by(|a, b| {
                match (a.1, b.1) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.0.cmp(&b.0),
                }
            });

            for (path, is_dir) in items {
                // Skip hidden files/directories
                if let Some(name) = path.file_name() {
                    if name.to_string_lossy().starts_with('.') {
                        continue;
                    }
                }

                // For app files, only show .newt and .md files (but always show directories)
                if is_app_file && !is_dir {
                    if !path.extension().map_or(false, |ext| ext == "newt" || ext == "md") {
                        continue;
                    }
                }

                let label = path.file_name().unwrap().to_string_lossy().to_string();
                let is_expanded = self.expanded_dirs.contains(&path);

                self.available_files.push(FileItem {
                    path: Some(path.clone()),
                    label,
                    is_header: false,
                    is_app_file,
                    is_directory: is_dir,
                    is_expanded,
                    depth,
                    parent_path: Some(dir.clone()),
                });

                // Recursively add children if directory is expanded
                if is_dir && is_expanded {
                    self.build_directory_tree(&path, depth + 1, is_app_file);
                }
            }
        }
    }

    fn toggle_directory(&mut self, index: usize) {
        if let Some(item) = self.available_files.get(index) {
            if item.is_directory {
                if let Some(path) = &item.path {
                    if self.expanded_dirs.contains(path) {
                        self.expanded_dirs.remove(path);
                    } else {
                        self.expanded_dirs.insert(path.clone());
                    }
                    self.refresh_file_list();
                }
            }
        }
    }

    fn collapse_directory(&mut self, index: usize) {
        if let Some(item) = self.available_files.get(index) {
            if item.is_directory && item.is_expanded {
                if let Some(path) = &item.path {
                    self.expanded_dirs.remove(path);
                    self.refresh_file_list();
                }
            } else if !item.is_directory && item.depth > 0 {
                // 'h' on a file: jump to parent directory
                if let Some(parent) = &item.parent_path {
                    for (i, file_item) in self.available_files.iter().enumerate() {
                        if file_item.path.as_ref() == Some(parent) {
                            self.file_list_state.select(Some(i));
                            break;
                        }
                    }
                }
            }
        }
    }

    fn expand_directory(&mut self, index: usize) {
        if let Some(item) = self.available_files.get(index) {
            if item.is_directory && !item.is_expanded {
                if let Some(path) = &item.path {
                    self.expanded_dirs.insert(path.clone());
                    self.refresh_file_list();
                }
            }
        }
    }

    fn perform_sidebar_search(&mut self) {
        self.sidebar_search.matches.clear();
        self.sidebar_search.current_match = None;

        if self.sidebar_search.query.is_empty() {
            return;
        }

        for (i, item) in self.available_files.iter().enumerate() {
            if !item.is_header {
                if self.sidebar_search.matches_pattern(&item.label) {
                    self.sidebar_search.matches.push(i);
                }
            }
        }

        if !self.sidebar_search.matches.is_empty() {
            self.sidebar_search.current_match = Some(0);
        }
    }

    fn perform_editor_search(&mut self) {
        self.editor_search.matches.clear();
        self.editor_search.current_match = None;

        if self.editor_search.query.is_empty() {
            return;
        }

        for (i, cell) in self.cells.iter().enumerate() {
            if self.editor_search.matches_pattern(&cell.content) {
                self.editor_search.matches.push(i);
            }
        }

        if !self.editor_search.matches.is_empty() {
            self.editor_search.current_match = Some(0);
        }
    }

    fn add_cell(&mut self, cell_type: CellType) {
        let is_shell = cell_type == CellType::Shell;
        self.insert_cell(self.cells.len(), cell_type);
        if is_shell {
            self.input_mode = InputMode::Editing;
            self.shell_cursor = 0;
        }
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
        self.shell_cursor = 0;
        self.input_mode = InputMode::Normal;
        self.dirty = true;
    }

    fn set_default_mode_for_selected_cell(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if let Some(cell) = self.cells.get(i) {
                if cell.cell_type == CellType::Shell {
                    self.input_mode = InputMode::Editing;
                    self.shell_cursor = cell.content.len();
                    return;
                }
            }
        }
        self.input_mode = InputMode::Normal;
        self.shell_cursor = 0;
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
        let mut path = if let Some(name) = filename {
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

        // Ensure .md extension for new files (unless it's .newt for backward compat)
        if filename.is_some() && !path.extension().map_or(false, |ext| ext == "md" || ext == "newt") {
            path.set_extension("md");
        }

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
            let notebook_path_str = self.file_path.as_ref().map(|p| p.to_string_lossy().to_string());

            Some(CommandRequest {
                command: cmd,
                language: lang,
                context: context_opt,
                client_type: Some("tui".to_string()),
                notebook_path: notebook_path_str,
            })
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
                if let Some(mode) = config.display_mode {
                    self.display_mode = mode;
                }
                if let Some(scheme) = config.colorscheme {
                    self.colorscheme = scheme;
                }
            }
        }
        // Set accent_color based on colorscheme
        self.accent_color = self.get_accent_color();
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
        config.display_mode = Some(self.display_mode.clone());
        config.colorscheme = Some(self.colorscheme.clone());

        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = fs::write(path, json);
        }
    }

    // Get accent color based on colorscheme
    fn get_accent_color(&self) -> Color {
        if self.colorscheme == "pastel" {
            Color::Indexed(183)  // Pastel purple/mauve
        } else {
            Color::Magenta  // ANSI magenta
        }
    }

    // Get markdown heading colors based on colorscheme
    fn get_h1_color(&self) -> Color {
        if self.colorscheme == "pastel" {
            Color::Indexed(117)  // Pastel blue
        } else {
            Color::Blue  // ANSI blue
        }
    }

    fn get_h2_color(&self) -> Color {
        if self.colorscheme == "pastel" {
            Color::Indexed(152)  // Pastel cyan
        } else {
            Color::Cyan  // ANSI cyan
        }
    }

    fn get_h3_color(&self) -> Color {
        if self.colorscheme == "pastel" {
            Color::Indexed(114)  // Pastel green
        } else {
            Color::Green  // ANSI green
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

fn prev_char_boundary(text: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    let clamped = idx.min(text.len());
    text[..clamped]
        .char_indices()
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn next_char_boundary(text: &str, idx: usize) -> usize {
    let clamped = idx.min(text.len());
    if clamped >= text.len() {
        return text.len();
    }

    let mut chars = text[clamped..].char_indices();
    let _ = chars.next();
    chars
        .next()
        .map(|(offset, _)| clamped + offset)
        .unwrap_or(text.len())
}

/// Detect the terminal prompt pattern from the file content
fn detect_terminal_prompt(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return None;
    }

    // Filter out empty lines
    let non_empty_lines: Vec<&str> = lines.iter()
        .filter(|l| !l.trim().is_empty())
        .copied()
        .collect();

    if non_empty_lines.is_empty() {
        return None;
    }

    let first_line = non_empty_lines[0];

    // Common prompt endings for different shells
    let prompt_chars = vec!['%', '$', '#', '>'];

    // For macOS/Unix terminals, look for patterns like:
    // username@hostname directory %
    // username@hostname directory $
    for &prompt_char in &prompt_chars {
        // Find the prompt character followed by a space
        let pattern = format!("{} ", prompt_char);
        if let Some(pos) = first_line.find(&pattern) {
            // Extract everything up to and including the prompt character and space
            let full_prompt = &first_line[..pos + 2];

            // For prompts like "username@hostname directory % ", we need to extract
            // the username part and prompt char, since directory changes
            // Look for username@hostname pattern
            if let Some(at_pos) = full_prompt.find('@') {
                let username = &full_prompt[..at_pos];

                // Check if this pattern (username + @ + prompt_char) appears in multiple lines
                let matches = non_empty_lines.iter()
                    .filter(|line| {
                        line.starts_with(username) &&
                        line.contains('@') &&
                        line.contains(prompt_char)
                    })
                    .count();

                if matches >= 2 {
                    // Return just the username@ part as a prefix pattern
                    // We'll match any line starting with this
                    return Some(username.to_string());
                }
            }

            // Fallback: check if the exact prompt appears multiple times
            let matches = non_empty_lines.iter()
                .filter(|line| line.starts_with(full_prompt))
                .count();

            if matches >= 2 {
                return Some(full_prompt.to_string());
            }
        }
    }

    None
}

/// Check if a line is a prompt line and extract the command
fn extract_command<'a>(line: &'a str, prompt: &str) -> Option<&'a str> {
    // For username-only patterns (e.g., "rohanadwankar")
    if !prompt.contains('@') && !prompt.contains(' ') {
        // Look for pattern: username@...promptchar command
        if line.starts_with(prompt) && line.contains('@') {
            // Find the prompt character (%, $, #, >)
            for prompt_char in &['%', '$', '#', '>'] {
                if let Some(pos) = line.find(&format!("{} ", prompt_char)) {
                    // Return the command part after "promptchar "
                    return Some(&line[pos + 2..]);
                }
            }
        }
    } else {
        // For full prompt patterns, use exact match
        if line.starts_with(prompt) {
            return Some(&line[prompt.len()..]);
        }
    }
    None
}

/// Convert terminal output to newt markdown cells
fn convert_terminal_to_markdown(content: &str, prompt: &str) -> String {
    let mut markdown = String::new();
    let lines: Vec<&str> = content.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        if let Some(command) = extract_command(line, prompt) {
            let command = command.trim();

            if !command.is_empty() {
                markdown.push_str(&format!("```sh\n{}\n```\n", command));

                // Collect output until the next prompt or end of file
                let mut output_lines = Vec::new();
                i += 1;

                while i < lines.len() && extract_command(lines[i], prompt).is_none() {
                    output_lines.push(lines[i]);
                    i += 1;
                }

                // Join output lines, trimming trailing empty lines
                while let Some(last) = output_lines.last() {
                    if last.trim().is_empty() {
                        output_lines.pop();
                    } else {
                        break;
                    }
                }

                if !output_lines.is_empty() {
                    // Use >> syntax for output
                    for output_line in output_lines {
                        markdown.push_str(&format!(">> {}\n", output_line));
                    }
                    markdown.push('\n');
                } else {
                    markdown.push('\n');
                }

                continue;
            }
        }

        i += 1;
    }

    markdown
}

/// Process terminal file: detect prompt, confirm with user, convert to markdown
fn process_terminal_file(file_path: &str, quiet: bool) -> Result<String, Box<dyn Error>> {
    use std::io::Write;

    // Read the terminal output file
    let content = fs::read_to_string(file_path)?;

    // Detect the terminal prompt
    let detected_prompt = detect_terminal_prompt(&content);

    let prompt = if quiet {
        // In quiet mode, use detected prompt or fail
        detected_prompt.ok_or("Could not detect terminal prompt. Please run without --quiet to specify manually.")?
    } else {
        // Ask user for confirmation
        if let Some(detected) = detected_prompt {
            print!("Detected terminal prompt: '{}'\nIs this correct? (press Enter to confirm, or type the correct prompt): ", detected);
            std::io::stdout().flush()?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim();

            if input.is_empty() {
                detected
            } else {
                input.to_string()
            }
        } else {
            print!("Could not detect terminal prompt. Please enter the prompt string: ");
            std::io::stdout().flush()?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim().to_string();

            if input.is_empty() {
                return Err("No prompt provided".into());
            }

            input
        }
    };

    // Convert to markdown
    let markdown = convert_terminal_to_markdown(&content, &prompt);

    // Create output filename (replace .txt with .md, or append .md)
    let output_path = if file_path.ends_with(".txt") {
        file_path.replace(".txt", ".md")
    } else {
        format!("{}.md", file_path)
    };

    // Write the markdown file
    fs::write(&output_path, markdown)?;

    Ok(output_path)
}

/// Run a markdown file (optionally filtering by heading)
async fn run_markdown_file(file_path: &str, heading: Option<&str>) -> Result<(), Box<dyn Error>> {
    // Read the markdown file
    let content = fs::read_to_string(file_path)?;

    // Parse cells from markdown
    let all_cells = markdown::parse_markdown(&content);

    // Filter cells by heading if specified
    let cells_to_run: Vec<Cell> = if let Some(target_heading) = heading {
        let mut filtered_cells = Vec::new();
        let mut in_target_section = false;
        let mut target_level: Option<usize> = None;

        for cell in all_cells {
            if cell.cell_type == CellType::Markdown {
                // Check if this markdown cell contains any headings
                for line in cell.content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with('#') {
                        // Parse heading level and text
                        let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                        let heading_text = trimmed[hash_count..].trim();

                        // Check if this is our target heading
                        if heading_text.eq_ignore_ascii_case(target_heading) {
                            in_target_section = true;
                            target_level = Some(hash_count);
                        } else if in_target_section {
                            // Check if we've reached a heading of the same or higher level
                            if let Some(target_lvl) = target_level {
                                if hash_count <= target_lvl {
                                    // End of target section
                                    in_target_section = false;
                                }
                            }
                        }
                    }
                }
            } else if in_target_section {
                // Add code cells within the target section
                filtered_cells.push(cell);
            }
        }

        if filtered_cells.is_empty() && heading.is_some() {
            eprintln!("Warning: No code blocks found under heading '{}'", target_heading);
        }

        filtered_cells
    } else {
        // Run all code cells
        all_cells.into_iter().filter(|c| c.cell_type != CellType::Markdown).collect()
    };

    if cells_to_run.is_empty() {
        println!("No code cells to execute.");
        return Ok(());
    }

    // Start server if not running
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

    for (i, cell) in cells_to_run.iter().enumerate() {
        let lang = match cell.cell_type {
            CellType::Rust => Some("rust".to_string()),
            CellType::Python => Some("python".to_string()),
            CellType::JavaScript => Some("javascript".to_string()),
            CellType::TypeScript => Some("typescript".to_string()),
            CellType::C => Some("c".to_string()),
            CellType::Cpp => Some("cpp".to_string()),
            CellType::Go => Some("go".to_string()),
            CellType::Shell => None,
            CellType::Markdown => continue,
        };

        let req = CommandRequest {
            command: cell.content.clone(),
            language: lang,
            context: None,
            client_type: Some("cli".to_string()),
            notebook_path: Some(file_path.to_string()),
        };

        println!("Running Cell {}/{} ({:?})", i + 1, cells_to_run.len(), cell.cell_type);

        let res = client.post("http://127.0.0.1:3030/exec")
            .json(&req)
            .send()
            .await;

        match res {
            Ok(resp) => {
                if let Ok(body) = resp.json::<CommandResponse>().await {
                    if !body.stdout.is_empty() {
                        print!("{}", body.stdout);
                    }
                    if !body.stderr.is_empty() {
                        eprint!("{}", body.stderr);
                    }
                } else {
                    eprintln!("Error parsing response");
                }
            }
            Err(e) => {
                eprintln!("Error executing cell: {}", e);
            }
        }
    }

    Ok(())
}

#[tokio::main]
pub async fn run() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Handle subcommands
    if let Some(command) = args.command {
        match command {
            Commands::Run { file, heading } => {
                return run_markdown_file(&file, heading.as_deref()).await;
            }
        }
    }

    if args.serve {
        println!("Starting server on http://127.0.0.1:3030");
        server::run_server().await;
        return Ok(());
    }

    // Handle --term flag for converting terminal output
    if let Some(term_file) = args.term {
        let output_path = process_terminal_file(&term_file, args.quiet)?;
        println!("Terminal output converted to: {}", output_path);

        // Continue to open the generated markdown file in the editor
        // by overriding the file_path argument
        let args = Args {
            file_path: Some(output_path),
            open: None,
            serve: false,
            term: None,
            quiet: args.quiet,
            command: None,
        };

        // Continue with normal flow below using the modified args
        return run_with_args(args).await;
    }

    run_with_args(args).await
}

async fn run_with_args(args: Args) -> Result<(), Box<dyn Error>> {

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

    // Handle positional file path argument
    let file_to_open = if let Some(file_path) = args.file_path {
        let path = std::path::PathBuf::from(&file_path);

        // If a directory is specified, change to it
        if let Some(parent) = path.parent() {
            if parent.as_os_str() != "" {
                std::env::set_current_dir(parent)?;
            }
        }

        // Get just the filename to pass to App::new
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
    } else {
        args.open
    };

    // Create app
    let mut app = App::new(file_to_open);

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

        // Periodic file tree refresh (every 2 seconds when sidebar is visible)
        if app.show_sidebar {
            let now = std::time::Instant::now();
            let refresh_interval = std::time::Duration::from_secs(2);
            if now.duration_since(app.last_file_refresh) >= refresh_interval {
                app.refresh_file_list();
                app.last_file_refresh = now;
            }
        }

        terminal.draw(|f| ui(f, &mut *app))?;

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
                        (' ', KeyCode::Char('h')) => {
                            // Space + h: Move focus to sidebar
                            if app.show_sidebar {
                                app.focus = Focus::Sidebar;
                            }
                            app.pending_key = None;
                            continue;
                        }
                        (' ', KeyCode::Char('l')) => {
                            // Space + l: Move focus to editor
                            app.focus = Focus::Editor;
                            app.pending_key = None;
                            continue;
                        }
                        ('g', KeyCode::Char('g')) => {
                            // gg - go to top (either sidebar or editor depending on focus)
                            if app.focus == Focus::Sidebar {
                                app.file_list_state.select(Some(0));
                            } else {
                                app.list_state.select(Some(0));
                            }
                            app.pending_key = None;
                            app.numeric_prefix = None;
                            continue;
                        }
                        _ => {
                            app.pending_key = None;
                            // if sequence fails, key is consumed.
                        }
                    }
                } else {
                    // Start sequence?
                    if app.input_mode == InputMode::Normal && key.code == KeyCode::Char(' ') {
                        app.pending_key = Some(' ');
                        continue;
                    }
                    // Start gg sequence
                    if app.input_mode == InputMode::Normal && key.code == KeyCode::Char('g') {
                        app.pending_key = Some('g');
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
                                    KeyCode::Char(c) if c.is_ascii_digit() => {
                                        if c == '0' && app.numeric_prefix.is_none() {
                                            // Jump to top
                                            app.file_list_state.select(Some(0));
                                        } else {
                                            let digit = c.to_digit(10).unwrap() as usize;
                                            app.numeric_prefix = Some(app.numeric_prefix.unwrap_or(0) * 10 + digit);
                                            app.numeric_prefix = Some(app.numeric_prefix.unwrap().min(9999));
                                        }
                                    }
                                    KeyCode::Char('j') | KeyCode::Down => {
                                        let count = app.numeric_prefix.unwrap_or(1);
                                        if let Some(i) = app.file_list_state.selected() {
                                            let new_pos = (i + count).min(app.available_files.len().saturating_sub(1));
                                            app.file_list_state.select(Some(new_pos));
                                        }
                                        app.numeric_prefix = None;
                                    }
                                    KeyCode::Char('k') | KeyCode::Up => {
                                        let count = app.numeric_prefix.unwrap_or(1);
                                        if let Some(i) = app.file_list_state.selected() {
                                            let new_pos = i.saturating_sub(count);
                                            app.file_list_state.select(Some(new_pos));
                                        }
                                        app.numeric_prefix = None;
                                    }
                                    KeyCode::Char('G') => {
                                        if let Some(count) = app.numeric_prefix {
                                            // Go to line number
                                            let new_pos = count.saturating_sub(1).min(app.available_files.len().saturating_sub(1));
                                            app.file_list_state.select(Some(new_pos));
                                        } else {
                                            // Go to last item
                                            app.file_list_state.select(Some(app.available_files.len().saturating_sub(1)));
                                        }
                                        app.numeric_prefix = None;
                                    }
                                    KeyCode::Enter => {
                                        if let Some(i) = app.file_list_state.selected() {
                                            let mut should_switch_focus = false;
                                            if i == 0 {
                                                // New Notebook
                                                app.cells.clear();
                                                app.add_cell(CellType::Shell);
                                                app.file_path = None;
                                                app.input_mode = InputMode::Normal;
                                                app.list_state.select(Some(0));
                                                should_switch_focus = true;
                                            } else {
                                                // Check if it's a directory or file
                                                if let Some(item) = app.available_files.get(i - 1) {
                                                    if item.is_directory {
                                                        // Toggle directory expansion - don't switch focus
                                                        app.toggle_directory(i - 1);
                                                    } else if item.is_header {
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
                                                            should_switch_focus = true;
                                                        } else {
                                                            // Open in external editor
                                                            disable_raw_mode()?;
                                                            execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;

                                                            let _ = run_external_editor(path, &app.editor);

                                                            execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
                                                            enable_raw_mode()?;
                                                            terminal.clear()?;
                                                            // External editor opened, but don't switch focus
                                                        }
                                                    }
                                                }
                                            }
                                            // Only switch focus if we opened a notebook
                                            if should_switch_focus {
                                                app.focus = Focus::Editor;
                                            }
                                        }
                                    }
                                    KeyCode::Char('h') | KeyCode::Left => {
                                        if let Some(i) = app.file_list_state.selected() {
                                            if i > 0 {
                                                app.collapse_directory(i - 1);
                                            }
                                        }
                                    }
                                    KeyCode::Char('l') | KeyCode::Right => {
                                        if let Some(i) = app.file_list_state.selected() {
                                            if i > 0 {
                                                if let Some(item) = app.available_files.get(i - 1) {
                                                    if item.is_directory {
                                                        app.expand_directory(i - 1);
                                                    }
                                                    // For files, l does nothing - use space+l to switch focus
                                                }
                                            }
                                        }
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
                                    KeyCode::Char('/') => {
                                        app.input_mode = InputMode::SearchSidebar;
                                        app.sidebar_search.clear();
                                    }
                                    KeyCode::Char('n') => {
                                        if let Some(current) = app.sidebar_search.current_match {
                                            if !app.sidebar_search.matches.is_empty() {
                                                let next = (current + 1) % app.sidebar_search.matches.len();
                                                app.sidebar_search.current_match = Some(next);
                                                if let Some(&idx) = app.sidebar_search.matches.get(next) {
                                                    app.file_list_state.select(Some(idx + 1)); // +1 for "New Notebook" offset
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('N') => {
                                        if let Some(current) = app.sidebar_search.current_match {
                                            if !app.sidebar_search.matches.is_empty() {
                                                let prev = if current == 0 {
                                                    app.sidebar_search.matches.len().saturating_sub(1)
                                                } else {
                                                    current - 1
                                                };
                                                app.sidebar_search.current_match = Some(prev);
                                                if let Some(&idx) = app.sidebar_search.matches.get(prev) {
                                                    app.file_list_state.select(Some(idx + 1)); // +1 for "New Notebook" offset
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char(':') => {
                                        app.input_mode = InputMode::Command;
                                        app.command_input.clear();
                                        // Clear search when entering command mode
                                        app.sidebar_search.clear();
                                        app.editor_search.clear();
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

                                                    // Also copy to system clipboard
                                                    match Clipboard::new() {
                                                        Ok(mut clipboard) => {
                                                            if clipboard.set_text(&cell.content).is_ok() {
                                                                app.status_message = Some("Cell yanked (copied to system clipboard)".to_string());
                                                            } else {
                                                                app.status_message = Some("Cell yanked (system clipboard failed)".to_string());
                                                            }
                                                        }
                                                        Err(_) => {
                                                            app.status_message = Some("Cell yanked (system clipboard unavailable)".to_string());
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('Y') => {
                                        if let Some(i) = app.list_state.selected() {
                                            if let Some(&cell_idx) = visual_items.get(i) {
                                                if let Some(cell) = app.cells.get(cell_idx) {
                                                    app.clipboard_output = Some(cell.output.clone());

                                                    // Also copy to system clipboard
                                                    match Clipboard::new() {
                                                        Ok(mut clipboard) => {
                                                            if clipboard.set_text(&cell.output).is_ok() {
                                                                app.status_message = Some("Output yanked (copied to system clipboard)".to_string());
                                                            } else {
                                                                app.status_message = Some("Output yanked (system clipboard failed)".to_string());
                                                            }
                                                        }
                                                        Err(_) => {
                                                            app.status_message = Some("Output yanked (system clipboard unavailable)".to_string());
                                                        }
                                                    }
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
                                    KeyCode::Char('/') => {
                                        app.input_mode = InputMode::SearchEditor;
                                        app.editor_search.clear();
                                    }
                                    KeyCode::Char('n') => {
                                        if let Some(current) = app.editor_search.current_match {
                                            if !app.editor_search.matches.is_empty() {
                                                let next = (current + 1) % app.editor_search.matches.len();
                                                app.editor_search.current_match = Some(next);
                                                if let Some(&idx) = app.editor_search.matches.get(next) {
                                                    app.list_state.select(Some(idx));
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('N') => {
                                        if let Some(current) = app.editor_search.current_match {
                                            if !app.editor_search.matches.is_empty() {
                                                let prev = if current == 0 {
                                                    app.editor_search.matches.len().saturating_sub(1)
                                                } else {
                                                    current - 1
                                                };
                                                app.editor_search.current_match = Some(prev);
                                                if let Some(&idx) = app.editor_search.matches.get(prev) {
                                                    app.list_state.select(Some(idx));
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char(':') => {
                                        app.input_mode = InputMode::Command;
                                        app.command_input.clear();
                                        // Clear search when entering command mode
                                        app.sidebar_search.clear();
                                        app.editor_search.clear();
                                    }
                                    KeyCode::Char(c) if c.is_ascii_digit() => {
                                        if c == '0' && app.numeric_prefix.is_none() {
                                            // Jump to top
                                            app.list_state.select(Some(0));
                                        } else {
                                            let digit = c.to_digit(10).unwrap() as usize;
                                            app.numeric_prefix = Some(app.numeric_prefix.unwrap_or(0) * 10 + digit);
                                            app.numeric_prefix = Some(app.numeric_prefix.unwrap().min(9999));
                                        }
                                    }
                                    KeyCode::Char('j') => {
                                        let count = app.numeric_prefix.unwrap_or(1);
                                        if let Some(i) = app.list_state.selected() {
                                            let new_pos = (i + count).min(visual_items.len().saturating_sub(1));
                                            app.list_state.select(Some(new_pos));
                                        }
                                        app.numeric_prefix = None;
                                    }
                                    KeyCode::Char('k') => {
                                        let count = app.numeric_prefix.unwrap_or(1);
                                        if let Some(i) = app.list_state.selected() {
                                            let new_pos = i.saturating_sub(count);
                                            app.list_state.select(Some(new_pos));
                                        }
                                        app.numeric_prefix = None;
                                    }
                                    KeyCode::Char('G') => {
                                        if let Some(count) = app.numeric_prefix {
                                            // Go to cell number
                                            let new_pos = count.saturating_sub(1).min(visual_items.len().saturating_sub(1));
                                            app.list_state.select(Some(new_pos));
                                        } else {
                                            // Go to last cell
                                            app.list_state.select(Some(visual_items.len().saturating_sub(1)));
                                        }
                                        app.numeric_prefix = None;
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
                                                        app.shell_cursor = content.len();
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
                                    KeyCode::Char('I') => {
                                        app.pending_delete = false;
                                        // View output in editor (read-only, changes discarded)
                                        if let Some(i) = app.list_state.selected() {
                                            if let Some(&cell_idx) = visual_items.get(i) {
                                                // Clone the output to view
                                                let (cell_type, output) = if let Some(cell) = app.cells.get(cell_idx) {
                                                    (cell.cell_type.clone(), cell.output.clone())
                                                } else {
                                                    continue;
                                                };

                                                // Determine file extension for syntax highlighting
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
                                                        let block = Block::default().borders(Borders::ALL).title("Viewing Output");
                                                        let text = Paragraph::new("Viewing output in external editor...\nChanges will be discarded.\nOr press Enter to force return.")
                                                            .block(block)
                                                            .alignment(ratatui::layout::Alignment::Center);
                                                        f.render_widget(text, popup_area);
                                                    })?;
                                                }

                                                // Open editor with output (result is discarded)
                                                let _res = open_editor(&output, ext, &editor_cmd, is_code);

                                                // Resume TUI
                                                if !is_code {
                                                    execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
                                                    enable_raw_mode()?;
                                                }
                                                terminal.clear()?; // Force redraw

                                                app.status_message = Some("Output viewed (changes discarded)".to_string());
                                            }
                                        }
                                    }
                                    KeyCode::Enter => {
                                         app.pending_delete = false;
                                         if let Some(i) = app.list_state.selected() {
                                            let cell_idx = i;
                                            
                                            // Open editor for Markdown cells
                                            if let Some(cell) = app.cells.get(cell_idx) {
                                                if cell.cell_type == CellType::Markdown {
                                                    // Clone needed data to avoid holding borrow
                                                    let content = cell.content.clone();
                                                    let ext = ".md";
                                                    
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
                                                    
                                                    continue;
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
                            if cmd.starts_with("ww ") {
                                // :ww filename - save to app directory
                                let mut filename = cmd[3..].trim().to_string();
                                if !filename.is_empty() {
                                    // Always use .md extension
                                    if !filename.ends_with(".md") && !filename.ends_with(".newt") {
                                        filename.push_str(".md");
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
                                        let path_str = path.to_string_lossy().to_string();
                                        app.save_notebook(Some(&path_str))?;
                                        app.status_message = Some(format!("Saved to {}", path.display()));
                                        app.input_mode = InputMode::Normal;
                                        app.refresh_file_list();
                                        app.command_input.clear();
                                    }
                                } else {
                                    app.input_mode = InputMode::Normal;
                                    app.command_input.clear();
                                }
                            } else if cmd.starts_with("w ") {
                                // :w filename - save to current directory
                                let mut filename = cmd[2..].trim().to_string();
                                if !filename.is_empty() {
                                    // Always use .md extension
                                    if !filename.ends_with(".md") && !filename.ends_with(".newt") {
                                        filename.push_str(".md");
                                    }
                                    let p = PathBuf::from(&filename);
                                    let path = if p.is_absolute() {
                                        p
                                    } else {
                                        std::env::current_dir()?.join(&filename)
                                    };

                                    if path.exists() {
                                        app.overwrite_path = Some(path.clone());
                                        app.input_mode = InputMode::ConfirmOverwrite;
                                        app.command_input = format!("Overwrite {}? y/N: ", path.display());
                                    } else {
                                        // Pass the full path string to save_notebook
                                        let path_str = path.to_string_lossy().to_string();
                                        app.save_notebook(Some(&path_str))?;
                                        app.status_message = Some(format!("Saved to {}", path.display()));
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
                            } else if cmd == "compact" {
                                app.display_mode = "compact".to_string();
                                app.save_config();
                                app.status_message = Some("Display mode set to compact".to_string());
                                app.input_mode = InputMode::Normal;
                                app.command_input.clear();
                            } else if cmd == "cozy" {
                                app.display_mode = "cozy".to_string();
                                app.save_config();
                                app.status_message = Some("Display mode set to cozy".to_string());
                                app.input_mode = InputMode::Normal;
                                app.command_input.clear();
                            } else if cmd.starts_with("colorscheme ") {
                                let scheme = cmd[12..].trim().to_string();
                                if scheme == "ansi" || scheme == "pastel" {
                                    app.colorscheme = scheme.clone();
                                    app.save_config();
                                    app.status_message = Some(format!("Colorscheme set to {}", scheme));
                                } else {
                                    app.status_message = Some("Invalid colorscheme. Use 'ansi' or 'pastel'".to_string());
                                }
                                app.input_mode = InputMode::Normal;
                                app.command_input.clear();
                            } else if let Ok(line_num) = cmd.parse::<usize>() {
                                // Jump to line number (1-indexed)
                                if line_num > 0 && line_num <= app.cells.len() {
                                    app.list_state.select(Some(line_num - 1)); // Convert to 0-indexed
                                    app.status_message = Some(format!("Jumped to cell {}", line_num));
                                } else if line_num == 0 {
                                    app.status_message = Some("Invalid line number: must be >= 1".to_string());
                                } else {
                                    app.status_message = Some(format!("Invalid line number: only {} cells", app.cells.len()));
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
                                        if let Some(cell) = app.cells.get_mut(i) {
                                            cell.cell_type = CellType::Rust;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "cpp" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i) {
                                            cell.cell_type = CellType::Cpp;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "c" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i) {
                                            cell.cell_type = CellType::C;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "py" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i) {
                                            cell.cell_type = CellType::Python;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "ts" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i) {
                                            cell.cell_type = CellType::TypeScript;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "js" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i) {
                                            cell.cell_type = CellType::JavaScript;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "md" | "markdown" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i) {
                                            cell.cell_type = CellType::Markdown;
                                            app.dirty = true;
                                        }
                                    }
                                    app.input_mode = InputMode::Normal;
                                }
                                "sh" | "shell" => {
                                    if let Some(i) = app.list_state.selected() {
                                        if let Some(cell) = app.cells.get_mut(i) {
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
                            let input = if let Some(i) = app.list_state.selected() {
                                if let Some(cell) = app.cells.get(i) {
                                    cell.content.trim().to_string()
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            };
                            let (new_type, ext) = match input.as_str() {
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
                                let mut is_interactive = false;
                                let mut cmd_to_run = String::new();

                                if let Some(cell) = app.current_cell_mut() {
                                    if cell.cell_type == CellType::Shell {
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
                            let cursor = app.shell_cursor;
                            if let Some(cell) = app.current_cell_mut() {
                                if cell.cell_type == CellType::Shell {
                                    let pos = cursor.min(cell.content.len());
                                    if cell.content.is_char_boundary(pos) {
                                        cell.content.insert(pos, c);
                                        app.shell_cursor = pos + c.len_utf8();
                                        app.dirty = true;
                                    }
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            let cursor = app.shell_cursor;
                            if let Some(cell) = app.current_cell_mut() {
                                if cell.cell_type == CellType::Shell && cursor > 0 {
                                    let prev = prev_char_boundary(&cell.content, cursor);
                                    if prev < cursor {
                                        cell.content.replace_range(prev..cursor, "");
                                        app.shell_cursor = prev;
                                        app.dirty = true;
                                    }
                                }
                            }
                        }
                        KeyCode::Delete => {
                            let cursor = app.shell_cursor;
                            if let Some(cell) = app.current_cell_mut() {
                                if cell.cell_type == CellType::Shell && cursor < cell.content.len() {
                                    let next = next_char_boundary(&cell.content, cursor);
                                    if cursor < next {
                                        cell.content.replace_range(cursor..next, "");
                                        app.dirty = true;
                                    }
                                }
                            }
                        }
                        KeyCode::Left => {
                            if let Some(i) = app.list_state.selected() {
                                if let Some(cell) = app.cells.get(i) {
                                    if cell.cell_type == CellType::Shell {
                                        app.shell_cursor = prev_char_boundary(&cell.content, app.shell_cursor.min(cell.content.len()));
                                    }
                                }
                            }
                        }
                        KeyCode::Right => {
                            if let Some(i) = app.list_state.selected() {
                                if let Some(cell) = app.cells.get(i) {
                                    if cell.cell_type == CellType::Shell {
                                        app.shell_cursor = next_char_boundary(&cell.content, app.shell_cursor.min(cell.content.len()));
                                    }
                                }
                            }
                        }
                        KeyCode::Home => {
                            if let Some(i) = app.list_state.selected() {
                                if let Some(cell) = app.cells.get(i) {
                                    if cell.cell_type == CellType::Shell {
                                        app.shell_cursor = 0;
                                    }
                                }
                            }
                        }
                        KeyCode::End => {
                            if let Some(i) = app.list_state.selected() {
                                if let Some(cell) = app.cells.get(i) {
                                    if cell.cell_type == CellType::Shell {
                                        app.shell_cursor = cell.content.len();
                                    }
                                }
                            }
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
                    },
                    InputMode::SearchSidebar => match key.code {
                        KeyCode::Enter => {
                            // Perform search
                            app.perform_sidebar_search();
                            // Jump to first match
                            if let Some(0) = app.sidebar_search.current_match {
                                if let Some(&idx) = app.sidebar_search.matches.get(0) {
                                    app.file_list_state.select(Some(idx + 1)); // +1 for "New Notebook" offset
                                }
                            }
                            app.input_mode = InputMode::Normal;
                        }
                        KeyCode::Esc => {
                            app.sidebar_search.clear();
                            app.input_mode = InputMode::Normal;
                        }
                        KeyCode::Char(c) => {
                            app.sidebar_search.query.push(c);
                            // Live search (update matches as user types)
                            app.perform_sidebar_search();
                        }
                        KeyCode::Backspace => {
                            app.sidebar_search.query.pop();
                            app.perform_sidebar_search();
                        }
                        _ => {}
                    },
                    InputMode::SearchEditor => match key.code {
                        KeyCode::Enter => {
                            app.perform_editor_search();
                            if let Some(0) = app.editor_search.current_match {
                                if let Some(&idx) = app.editor_search.matches.get(0) {
                                    app.list_state.select(Some(idx));
                                }
                            }
                            app.input_mode = InputMode::Normal;
                        }
                        KeyCode::Esc => {
                            app.editor_search.clear();
                            app.input_mode = InputMode::Normal;
                        }
                        KeyCode::Char(c) => {
                            app.editor_search.query.push(c);
                            app.perform_editor_search();
                        }
                        KeyCode::Backspace => {
                            app.editor_search.query.pop();
                            app.perform_editor_search();
                        }
                        _ => {}
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

fn highlight_code(code: &str, cell_type: &CellType, colorscheme: &str) -> Vec<Line<'static>> {
    let syntax_name = match cell_type {
        CellType::Rust => "Rust",
        CellType::Python => "Python",
        CellType::JavaScript => "JavaScript",
        CellType::TypeScript => "TypeScript",
        CellType::C => "C",
        CellType::Cpp => "C++",
        CellType::Go => "Go",
        CellType::Shell => "Bash",
        CellType::Markdown => {
            return code.lines().map(|line| Line::from(line.to_string())).collect();
        }
    };

    let syntax = SYNTAX_SET.find_syntax_by_name(syntax_name)
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    // Use Solarized (dark) for better colors, fall back to base16-ocean.dark
    let theme_key = if THEME_SET.themes.contains_key("Solarized (dark)") {
        "Solarized (dark)"
    } else {
        "base16-ocean.dark"
    };
    let theme = &THEME_SET.themes[theme_key];
    let mut highlighter = HighlightLines::new(syntax, theme);

    code.lines().map(|line| {
        let ranges = highlighter.highlight_line(line, &SYNTAX_SET).unwrap_or_default();
        let spans: Vec<Span> = ranges.iter().map(|(style, text)| {
            let fg = style.foreground;
            // Convert RGB to color based on colorscheme
            let color = if colorscheme == "pastel" {
                rgb_to_pastel_color(fg.r, fg.g, fg.b)
            } else {
                rgb_to_ansi_color(fg.r, fg.g, fg.b)
            };
            Span::styled(text.to_string(), Style::default().fg(color))
        }).collect();
        Line::from(spans)
    }).collect()
}

// Convert RGB color to nearest ANSI color
fn rgb_to_ansi_color(r: u8, g: u8, b: u8) -> Color {
    // Calculate brightness
    let brightness = (r as u32 + g as u32 + b as u32) / 3;

    // Determine which color component is dominant
    let max_component = r.max(g).max(b);
    let min_component = r.min(g).min(b);
    let saturation = if max_component > 0 {
        ((max_component - min_component) as f32 / max_component as f32) * 100.0
    } else {
        0.0
    };

    // If low saturation, it's a grey/white/black
    if saturation < 20.0 {
        if brightness < 85 {
            Color::DarkGray
        } else if brightness < 170 {
            Color::Gray
        } else {
            Color::White
        }
    } else {
        // Determine the hue based on which component is dominant
        if r > g && r > b {
            if brightness > 128 { Color::LightRed } else { Color::Red }
        } else if g > r && g > b {
            if brightness > 128 { Color::LightGreen } else { Color::Green }
        } else if b > r && b > g {
            if brightness > 128 { Color::LightBlue } else { Color::Blue }
        } else if r > b && g > b {
            if brightness > 128 { Color::LightYellow } else { Color::Yellow }
        } else if r > g && b > g {
            if brightness > 128 { Color::LightMagenta } else { Color::Magenta }
        } else {
            if brightness > 128 { Color::LightCyan } else { Color::Cyan }
        }
    }
}

// Convert RGB color to pastel indexed color
fn rgb_to_pastel_color(r: u8, g: u8, b: u8) -> Color {
    // Calculate brightness
    let brightness = (r as u32 + g as u32 + b as u32) / 3;

    // Determine which color component is dominant
    let max_component = r.max(g).max(b);
    let min_component = r.min(g).min(b);
    let saturation = if max_component > 0 {
        ((max_component - min_component) as f32 / max_component as f32) * 100.0
    } else {
        0.0
    };

    // If low saturation, it's a grey
    if saturation < 20.0 {
        // Use light grey from 256-color palette
        if brightness < 85 {
            Color::Indexed(244)  // Dark grey
        } else if brightness < 170 {
            Color::Indexed(250)  // Medium grey
        } else {
            Color::Indexed(253)  // Light grey
        }
    } else {
        // Determine the hue based on which component is dominant
        // Use pastel indexed colors from the 256-color palette
        if r > g && r > b {
            // Red/Pink tones
            if brightness > 128 {
                Color::Indexed(217)  // Light pink
            } else {
                Color::Indexed(211)  // Pastel red
            }
        } else if g > r && g > b {
            // Green tones
            if brightness > 128 {
                Color::Indexed(156)  // Light pastel green
            } else {
                Color::Indexed(114)  // Pastel green
            }
        } else if b > r && b > g {
            // Blue tones
            if brightness > 128 {
                Color::Indexed(153)  // Light pastel blue
            } else {
                Color::Indexed(117)  // Pastel blue
            }
        } else if r > b && g > b {
            // Yellow tones
            if brightness > 128 {
                Color::Indexed(229)  // Light pastel yellow
            } else {
                Color::Indexed(222)  // Pastel yellow
            }
        } else if r > g && b > g {
            // Magenta/Purple tones
            if brightness > 128 {
                Color::Indexed(219)  // Light pastel magenta
            } else {
                Color::Indexed(182)  // Pastel magenta
            }
        } else {
            // Cyan tones
            if brightness > 128 {
                Color::Indexed(159)  // Light pastel cyan
            } else {
                Color::Indexed(152)  // Pastel cyan
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
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
        for (idx, item) in app.available_files.iter().enumerate() {
            if item.is_header {
                items.push(ListItem::new(Span::styled(format!("--- {} ---", item.label), Style::default().fg(Color::DarkGray))));
            } else {
                // Build indentation based on depth
                let indent = "  ".repeat(item.depth);

                // Directory indicator
                let indicator = if item.is_directory {
                    if item.is_expanded { "▾ " } else { "▸ " }
                } else {
                    "  "
                };

                let name = if let Some(path) = &item.path {
                    path.file_name().unwrap().to_string_lossy().to_string()
                } else {
                    item.label.clone()
                };

                let display_name = format!("{}{}{}", indent, indicator, name);

                // Highlight if matches search
                let mut style = Style::default();
                if (app.input_mode == InputMode::SearchSidebar || !app.sidebar_search.matches.is_empty()) && app.sidebar_search.matches.contains(&idx) {
                    style = style.fg(Color::Yellow);
                    if Some(&idx) == app.sidebar_search.current_match.and_then(|cm| app.sidebar_search.matches.get(cm)) {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                }

                items.push(ListItem::new(display_name).style(style));
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
                 if app.display_mode == "compact" {
                     if cell.content.is_empty() {
                         cell_lines.push(Line::from(vec![
                            Span::raw("(empty)"),
                            Span::raw(" ".repeat(width.saturating_sub(7 + header_str.len()))),
                            Span::styled(header_str.clone(), style),
                        ]));
                     } else {
                         let mut lines_iter = cell.content.lines();
                         if let Some(first_line) = lines_iter.next() {
                             let first_padding_len = width.saturating_sub(first_line.len() + header_str.len()).saturating_sub(2);
                             let first_padding = " ".repeat(first_padding_len);
                             cell_lines.push(Line::from(vec![
                                Span::raw(first_line),
                                Span::raw(first_padding),
                                Span::styled(header_str.clone(), style),
                            ]));
                             for line in lines_iter {
                                 cell_lines.push(Line::from(line));
                             }
                         }
                     }
                 } else {
                     // Cozy mode: original rendering
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
                             cell_lines.push(Line::from(Span::styled(line, Style::default().fg(app.get_h1_color()).add_modifier(Modifier::BOLD))));
                         } else if line.starts_with("## ") {
                             cell_lines.push(Line::from(Span::styled(line, Style::default().fg(app.get_h2_color()).add_modifier(Modifier::BOLD))));
                         } else if line.starts_with("### ") {
                             cell_lines.push(Line::from(Span::styled(line, Style::default().fg(app.get_h3_color()).add_modifier(Modifier::BOLD))));
                         } else if line.starts_with("- ") || line.starts_with("* ") {
                             cell_lines.push(Line::from(format!("  • {}", &line[2..])));
                         } else {
                             cell_lines.push(Line::from(line));
                         }
                     }
                 }
             }
        } else {
             if app.input_mode == InputMode::Editing && is_selected && cell.cell_type == CellType::Shell {
                 let cursor = app.shell_cursor.min(cell.content.len());
                 let before = &cell.content[..cursor];
                 let after = &cell.content[cursor..];
                 let (cursor_char, after_rest) = if let Some(ch) = after.chars().next() {
                     (ch.to_string(), after[ch.len_utf8()..].to_string())
                 } else {
                     (" ".to_string(), String::new())
                 };

                 if app.display_mode == "compact" {
                     let mut first_line = Line::from(vec![
                        Span::raw(before.to_string()),
                        Span::styled(cursor_char, Style::default().add_modifier(Modifier::REVERSED)),
                        Span::raw(after_rest),
                    ]);

                     let first_line_len = cell.content.chars().count().saturating_add(1);
                     let first_padding_len = width.saturating_sub(first_line_len + header_str.len()).saturating_sub(2);
                     let first_padding = " ".repeat(first_padding_len);
                     first_line.spans.push(Span::raw(first_padding));
                     first_line.spans.push(Span::styled(header_str.clone(), style));
                     cell_lines.push(first_line);
                 } else {
                     cell_lines.push(Line::from(vec![
                        Span::styled("In:", style),
                        Span::raw(padding.clone()),
                        Span::styled(header_str.clone(), style),
                    ]));

                     cell_lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::raw(before.to_string()),
                        Span::styled(cursor_char, Style::default().add_modifier(Modifier::REVERSED)),
                        Span::raw(after_rest),
                    ]));
                 }
             } else {
             if app.display_mode == "compact" {
                 // Compact mode: first line with tag, no "In:" label, with syntax highlighting
                 if cell.content.is_empty() {
                     cell_lines.push(Line::from(vec![
                        Span::raw("(empty)"),
                        Span::raw(" ".repeat(width.saturating_sub(7 + header_str.len()))),
                        Span::styled(header_str.clone(), style),
                    ]));
                 } else {
                     let highlighted_lines = highlight_code(&cell.content, &cell.cell_type, &app.colorscheme);
                     if let Some(mut first_line) = highlighted_lines.first().cloned() {
                         let first_line_len = first_line.spans.iter().map(|s| s.content.len()).sum::<usize>();
                         let first_padding_len = width.saturating_sub(first_line_len + header_str.len()).saturating_sub(2);
                         let first_padding = " ".repeat(first_padding_len);
                         first_line.spans.push(Span::raw(first_padding));
                         first_line.spans.push(Span::styled(header_str.clone(), style));
                         cell_lines.push(first_line);
                         for line in highlighted_lines.into_iter().skip(1) {
                             cell_lines.push(line);
                         }
                     }
                 }
             } else {
                 // Cozy mode: original rendering with syntax highlighting
                 cell_lines.push(Line::from(vec![
                    Span::styled("In:", style),
                    Span::raw(padding),
                    Span::styled(header_str, style),
                ]));
                 if cell.content.is_empty() {
                     cell_lines.push(Line::from("     (empty)"));
                 } else {
                     let highlighted_lines = highlight_code(&cell.content, &cell.cell_type, &app.colorscheme);
                     for mut line in highlighted_lines {
                         line.spans.insert(0, Span::raw("     "));
                         cell_lines.push(line);
                     }
                 }
             }
             }
        };
        
        // Output Section (combined)
        if cell.cell_type != CellType::Markdown {
            let output_style = if is_selected {
                Style::default().fg(app.accent_color).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            if app.display_mode == "compact" {
                // Compact mode: use ">>" prefix, no "Out:" label
                if !cell.output.is_empty() {
                    let lines: Vec<&str> = cell.output.lines().collect();
                    if lines.len() > 10 {
                        for line in lines.iter().take(10) {
                            cell_lines.push(Line::from(format!(">>  {}", line)));
                        }
                        cell_lines.push(Line::from(">>  ....."));
                    } else {
                        for line in lines {
                            cell_lines.push(Line::from(format!(">>  {}", line)));
                        }
                    }
                }
            } else {
                // Cozy mode: original rendering with "Out:" label
                cell_lines.push(Line::from("")); // Spacer between input and output

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
        }

        cell_lines.push(Line::from("")); // Bottom spacer
        list_items.push(ListItem::new(cell_lines));
    }

    // Simple offset rule: ensure prev, current, and next cells are visible
    // This guarantees context and lets ratatui fill the viewport naturally
    if let Some(selected) = app.list_state.selected() {
        // Show previous cell if it exists (for context)
        let offset = if selected > 0 {
            selected - 1
        } else {
            0
        };
        *app.list_state.offset_mut() = offset;
    }

    let list = List::new(list_items)
        .block(Block::default().borders(Borders::NONE))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    f.render_stateful_widget(list, editor_area, &mut app.list_state.clone());

    // Bottom bar
    match app.input_mode {
        InputMode::Editing => {
            let input_block = Paragraph::new("-- INSERT --")
                .style(Style::default().fg(app.accent_color).add_modifier(Modifier::BOLD));
            f.render_widget(input_block, bottom_bar_area);
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

                // Show search count on the right if there's an active search
                let search_info = if app.focus == Focus::Sidebar && !app.sidebar_search.query.is_empty() {
                    if !app.sidebar_search.matches.is_empty() {
                        format!(" /{} ({}/{})",
                            app.sidebar_search.query,
                            app.sidebar_search.current_match.map(|m| m + 1).unwrap_or(0),
                            app.sidebar_search.matches.len())
                    } else {
                        format!(" /{} (no matches)", app.sidebar_search.query)
                    }
                } else if app.focus == Focus::Editor && !app.editor_search.query.is_empty() {
                    if !app.editor_search.matches.is_empty() {
                        format!(" /{} ({}/{})",
                            app.editor_search.query,
                            app.editor_search.current_match.map(|m| m + 1).unwrap_or(0),
                            app.editor_search.matches.len())
                    } else {
                        format!(" /{} (no matches)", app.editor_search.query)
                    }
                } else {
                    String::new()
                };

                // Add cell position indicator on the right (like vim)
                let position_info = if app.focus == Focus::Editor && !app.cells.is_empty() {
                    if let Some(selected) = app.list_state.selected() {
                        let total = app.cells.len();
                        let current = selected + 1; // 1-indexed for display
                        let percentage = (current * 100) / total;
                        format!(" {}% < {}", percentage, current)
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                // Build left side (status + search info) and right side (position info)
                let left_text = format!("{}{}", status, search_info);
                let right_text = position_info;

                // Calculate spacing to right-align position info
                let left_len = left_text.chars().count();
                let right_len = right_text.chars().count();
                let total_width = bottom_bar_area.width as usize;

                let spacing = if left_len + right_len < total_width {
                    " ".repeat(total_width - left_len - right_len)
                } else {
                    String::new()
                };

                let full_status = format!("{}{}{}", left_text, spacing, right_text);
                let status_block = Paragraph::new(full_status)
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
        InputMode::SearchSidebar => {
            let search_text = format!("/{}", app.sidebar_search.query);
            let match_info = if !app.sidebar_search.matches.is_empty() {
                format!(" ({}/{})",
                    app.sidebar_search.current_match.map(|m| m + 1).unwrap_or(0),
                    app.sidebar_search.matches.len())
            } else if !app.sidebar_search.query.is_empty() {
                " (no matches)".to_string()
            } else {
                String::new()
            };

            let input_block = Paragraph::new(format!("{}{}", search_text, match_info))
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(input_block, bottom_bar_area);
        }
        InputMode::SearchEditor => {
            let search_text = format!("/{}", app.editor_search.query);
            let match_info = if !app.editor_search.matches.is_empty() {
                format!(" ({}/{})",
                    app.editor_search.current_match.map(|m| m + 1).unwrap_or(0),
                    app.editor_search.matches.len())
            } else if !app.editor_search.query.is_empty() {
                " (no matches)".to_string()
            } else {
                String::new()
            };

            let input_block = Paragraph::new(format!("{}{}", search_text, match_info))
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(input_block, bottom_bar_area);
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
