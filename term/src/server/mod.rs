use axum::{
    Json, Router,
    routing::{get, post},
    extract::Request,
    middleware::{self, Next},
    response::Response,
};
use directories::ProjectDirs;
use ignore::gitignore::Gitignore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;
use lazy_static::lazy_static;

pub mod kernel;
use tower_http::cors::{CorsLayer, Any};

use crate::{CommandRequest, CommandResponse, CellType, Notebook, ExportResponse};

struct ShellSession {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    marker_counter: u64,
}

impl ShellSession {
    fn new() -> Result<Self, String> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

        let mut child = Command::new(&shell)
            .arg("-l")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start shell '{}': {}", shell, e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to capture shell stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture shell stdout".to_string())?;

        let mut session = Self {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout),
            marker_counter: 0,
        };

        session
            .stdin
            .write_all(b"exec 2>&1\n")
            .map_err(|e| format!("Failed to configure shell stderr redirection: {}", e))?;
        session
            .stdin
            .flush()
            .map_err(|e| format!("Failed to flush shell init command: {}", e))?;

        Ok(session)
    }

    fn run_command(&mut self, command: &str) -> Result<(String, i32), String> {
        self.marker_counter = self.marker_counter.wrapping_add(1);
        let marker = format!("__NEWT_STATUS_MARKER_{}__", self.marker_counter);

        self.stdin
            .write_all(command.as_bytes())
            .map_err(|e| format!("Failed writing command to shell: {}", e))?;
        self.stdin
            .write_all(b"\n")
            .map_err(|e| format!("Failed writing command newline to shell: {}", e))?;

        let marker_cmd = format!("printf '\n{}%s\n' \"$?\"\n", marker);
        self.stdin
            .write_all(marker_cmd.as_bytes())
            .map_err(|e| format!("Failed writing status marker command: {}", e))?;
        self.stdin
            .flush()
            .map_err(|e| format!("Failed flushing shell input: {}", e))?;

        let mut output = String::new();
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = self
                .stdout
                .read_line(&mut line)
                .map_err(|e| format!("Failed reading shell output: {}", e))?;

            if bytes_read == 0 {
                return Err("Shell session closed unexpectedly".to_string());
            }

            let trimmed = line.trim_end_matches(['\r', '\n']);
            if let Some(status_str) = trimmed.strip_prefix(&marker) {
                let status = status_str.trim().parse::<i32>().unwrap_or(1);
                return Ok((output, status));
            }

            output.push_str(&line);
        }
    }
}

lazy_static! {
    static ref SHELL_SESSION: Mutex<Option<ShellSession>> = Mutex::new(None);
}

#[derive(Serialize, Deserialize)]
pub struct InputStatusResponse {
    pub required: bool,
    pub prompt: String,
}

#[derive(Serialize, Deserialize)]
pub struct InputSubmitRequest {
    pub value: String,
}

//TODO: need a better way for newt cloud to operate with file system than this lol. i think the
//right approach is to in core-api it should save which directory it is started from and then
//whenever either of the frontends try to connect with it then it will use that as the base dir.
//from there the frontends can travel up and down directories as needed throught the file tree but
//we should generally avoid this sort of logic
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

pub async fn execute_command(Json(payload): Json<CommandRequest>) -> Json<CommandResponse> {
    // Intentionally no stdout logging here: server runs under the TUI.
    let language = payload.language.as_deref().unwrap_or("shell");

    let response = match language {
        "rust" => execute_rust(payload.command, payload.context).await,
        "python" => execute_python(payload.command, payload.context, payload.client_type, payload.notebook_path).await,
        "javascript" => execute_javascript(payload.command, payload.context),
        "typescript" => execute_typescript(payload.command, payload.context).await,
        "c" => execute_c(payload.command, payload.context).await,
        "cpp" => execute_cpp(payload.command, payload.context).await,
        "go" => execute_go(payload.command, payload.context).await,
        _ => execute_shell(payload.command).await,
    };
    response
}

pub async fn export_notebook(Json(notebook): Json<Notebook>) -> Json<ExportResponse> {
    let mut markdown = String::from("# Newt Notebook Export\n\n");

    for (i, cell) in notebook.cells.iter().enumerate() {
        if cell.cell_type == CellType::Markdown {
            markdown.push_str(&cell.content);
            markdown.push_str("\n\n");
            continue;
        }

        let lang_str = match cell.cell_type {
            CellType::Shell => "bash",
            CellType::Rust => "rust",
            CellType::Python => "python",
            CellType::JavaScript => "javascript",
            CellType::TypeScript => "typescript",
            CellType::C => "c",
            CellType::Cpp => "cpp",
            CellType::Go => "go",
            CellType::Markdown => "markdown",
        };

        markdown.push_str(&format!("## Cell {} ({})\n", i + 1, lang_str));
        markdown.push_str("```");
        markdown.push_str(lang_str);
        markdown.push_str("\n");
        markdown.push_str(&cell.content);
        markdown.push_str("\n```\n");

        if !cell.output.is_empty() {
            markdown.push_str("### Output\n");
            markdown.push_str("```\n");
            markdown.push_str(&cell.output);
            markdown.push_str("\n```\n");
        }
        markdown.push_str("\n");
    }

    Json(ExportResponse { markdown })
}

async fn execute_shell(command: String) -> Json<CommandResponse> {
    if command.trim().is_empty() {
        return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: "".to_string(),
            status: Some(0),
            display_data: None,
        });
    }

    tokio::task::spawn_blocking(move || {
        let mut shell_guard = match SHELL_SESSION.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return Json(CommandResponse {
                    stdout: "".to_string(),
                    stderr: "Failed to lock shell session mutex".to_string(),
                    status: Some(1),
                    display_data: None,
                })
            }
        };

        if shell_guard.is_none() {
            match ShellSession::new() {
                Ok(session) => *shell_guard = Some(session),
                Err(e) => {
                    return Json(CommandResponse {
                        stdout: "".to_string(),
                        stderr: e,
                        status: Some(1),
                        display_data: None,
                    })
                }
            }
        }

        let run_result = shell_guard
            .as_mut()
            .unwrap()
            .run_command(&command);

        match run_result {
            Ok((stdout, status)) => Json(CommandResponse {
                stdout,
                stderr: "".to_string(),
                status: Some(status),
                display_data: None,
            }),
            Err(first_error) => {
                // If session died, try one restart before failing.
                *shell_guard = None;
                let retry = ShellSession::new().and_then(|mut session| {
                    let result = session.run_command(&command);
                    if result.is_ok() {
                        *shell_guard = Some(session);
                    }
                    result
                });

                match retry {
                    Ok((stdout, status)) => Json(CommandResponse {
                        stdout,
                        stderr: "".to_string(),
                        status: Some(status),
                        display_data: None,
                    }),
                    Err(second_error) => Json(CommandResponse {
                        stdout: "".to_string(),
                        stderr: format!(
                            "Shell session error: {}. Retry failed: {}",
                            first_error, second_error
                        ),
                        status: Some(1),
                        display_data: None,
                    }),
                }
            }
        }
    })
    .await
    .unwrap_or_else(|e| Json(CommandResponse {
        stdout: "".to_string(),
        stderr: format!("Shell task failed: {}", e),
        status: Some(1),
        display_data: None,
    }))
}

async fn execute_rust(code: String, context: Option<Vec<String>>) -> Json<CommandResponse> {
    tokio::task::spawn_blocking(move || {
        use crate::server::kernel::{self, Kernel};
        match kernel::get_or_init_rust_kernel() {
            Ok(mut kernel_guard) => {
                if let Some(kernel) = kernel_guard.as_mut() {
                    match kernel.execute(code, None, context, None) {
                        Ok(response) => Json(CommandResponse {
                            stdout: response.stdout,
                            stderr: response.stderr,
                            status: response.status,
                            display_data: response.display_data,
                        }),
                        Err(e) => Json(CommandResponse {
                            stdout: "".to_string(),
                            stderr: format!("Kernel execution failed: {}", e),
                            status: Some(1),
                            display_data: None,
                        }),
                    }
                } else {
                    Json(CommandResponse {
                        stdout: "".to_string(),
                        stderr: "Kernel not initialized".to_string(),
                        status: Some(1),
                        display_data: None,
                    })
                }
            }
            Err(e) => Json(CommandResponse {
                stdout: "".to_string(),
                stderr: format!("Failed to access kernel: {}", e),
                status: Some(1),
                display_data: None,
            }),
        }
    }).await.unwrap_or_else(|e| Json(CommandResponse {
        stdout: "".to_string(),
        stderr: format!("Task join error: {}", e),
        status: Some(1),
        display_data: None,
    }))
}



async fn execute_python(code: String, context: Option<Vec<String>>, client_type: Option<String>, notebook_path: Option<String>) -> Json<CommandResponse> {
    tokio::task::spawn_blocking(move || {
        use crate::server::kernel::{self, Kernel};
        // Try to use persistent kernel
        match kernel::get_or_init_python_kernel(notebook_path) {
            Ok(mut kernel_guard) => {
                if let Some(kernel) = kernel_guard.as_mut() {
                    match kernel.execute(code, None, context, client_type) {
                        Ok(response) => {
                            return Json(CommandResponse {
                                stdout: response.stdout,
                                stderr: response.stderr,
                                status: response.status,
                                display_data: response.display_data,
                            });
                        }
                        Err(e) => {
                            return Json(CommandResponse {
                                stdout: "".to_string(),
                                stderr: format!("Kernel execution failed: {}", e),
                                status: Some(1),
                                display_data: None,
                            });
                        }
                    }
                } else {
                    return Json(CommandResponse {
                        stdout: "".to_string(),
                        stderr: "Kernel not initialized".to_string(),
                        status: Some(1),
                        display_data: None,
                    });
                }
            }
            Err(e) => {
                return Json(CommandResponse {
                    stdout: "".to_string(),
                    stderr: format!("Failed to access kernel: {}", e),
                    status: Some(1),
                    display_data: None,
                });
            }
        }
    }).await.unwrap_or_else(|e| {
        Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Task join error: {}", e),
            status: Some(1),
            display_data: None,
        })
    })
}



fn execute_javascript(code: String, context: Option<Vec<String>>) -> Json<CommandResponse> {
    use crate::server::kernel::{self, Kernel};
    match kernel::get_or_init_node_kernel() {
        Ok(mut kernel_guard) => {
            if let Some(kernel) = kernel_guard.as_mut() {
                match kernel.execute(code, None, context, None) {
                    Ok(response) => Json(CommandResponse {
                        stdout: response.stdout,
                        stderr: response.stderr,
                        status: response.status,
                        display_data: response.display_data,
                    }),
                    Err(e) => Json(CommandResponse {
                        stdout: "".to_string(),
                        stderr: format!("Kernel execution failed: {}", e),
                        status: Some(1),
                        display_data: None,
                    }),
                }
            } else {
                Json(CommandResponse {
                    stdout: "".to_string(),
                    stderr: "Kernel not initialized".to_string(),
                    status: Some(1),
                    display_data: None,
                })
            }
        }
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to access kernel: {}", e),
            status: Some(1),
            display_data: None,
        }),
    }
}

async fn execute_typescript(code: String, context: Option<Vec<String>>) -> Json<CommandResponse> {
    // TypeScript still uses the old stateless way for now, or we could transpile and send to NodeKernel
    // For now, keep stateless to ensure it works.
    
    // Polyfill for prompt/input
    let polyfill = r#"
const fs = require('fs');
const os = require('os');
const path = require('path');

function _newt_prompt(promptText) {
    promptText = promptText || "";
    const tempDir = os.tmpdir();
    const reqPath = path.join(tempDir, "newt_web_input_req");
    const resPath = path.join(tempDir, "newt_web_input_res");

    if (fs.existsSync(resPath)) {
        try { fs.unlinkSync(resPath); } catch(e) {}
    }

    fs.writeFileSync(reqPath, promptText);

    const sab = new SharedArrayBuffer(4);
    const int32 = new Int32Array(sab);
    const startTime = Date.now();
    
    while (!fs.existsSync(resPath)) {
        if (Date.now() - startTime > 300000) break;
        try {
            Atomics.wait(int32, 0, 0, 100);
        } catch (e) {
             const start = Date.now();
             while (Date.now() - start < 100) {}
        }
    }

    let result = "";
    if (fs.existsSync(resPath)) {
        result = fs.readFileSync(resPath, 'utf8');
    }

    try {
        if (fs.existsSync(reqPath)) fs.unlinkSync(reqPath);
        if (fs.existsSync(resPath)) fs.unlinkSync(resPath);
    } catch(e) {}

    return result;
}
global.prompt = _newt_prompt;
global.input = _newt_prompt;
"#;

    let full_code = if let Some(ctx) = context {
        format!("{}\n{}\n{}", polyfill, ctx.join("\n"), code)
    } else {
        format!("{}\n{}", polyfill, code)
    };

    let temp_dir = std::env::temp_dir();
    let file_name = format!("newt_script_{}.ts", uuid::Uuid::new_v4());
    let file_path = temp_dir.join(&file_name);

    if let Err(e) = std::fs::write(&file_path, &full_code) {
        return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to write source file: {}", e),
            status: None,
            display_data: None,
        });
    }

    // Try npx tsx first
    let output = tokio::process::Command::new("npx").arg("tsx").arg(&file_path).output().await;

    let _ = std::fs::remove_file(&file_path);

    match output {
        Ok(output) => Json(CommandResponse {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status: output.status.code(),
            display_data: None,
        }),
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to execute npx tsx: {}", e),
            status: None,
            display_data: None,
        }),
    }
}

async fn execute_c(code: String, context: Option<Vec<String>>) -> Json<CommandResponse> {
    tokio::task::spawn_blocking(move || {
        use crate::server::kernel::{self, Kernel};
        match kernel::get_or_init_c_kernel() {
            Ok(mut kernel_guard) => {
                if let Some(kernel) = kernel_guard.as_mut() {
                    match kernel.execute(code, None, context, None) {
                        Ok(response) => Json(CommandResponse {
                            stdout: response.stdout,
                            stderr: response.stderr,
                            status: response.status,
                            display_data: response.display_data,
                        }),
                        Err(e) => Json(CommandResponse {
                            stdout: "".to_string(),
                            stderr: format!("Kernel execution failed: {}", e),
                            status: Some(1),
                            display_data: None,
                        }),
                    }
                } else {
                    Json(CommandResponse {
                        stdout: "".to_string(),
                        stderr: "Kernel not initialized".to_string(),
                        status: Some(1),
                        display_data: None,
                    })
                }
            }
            Err(e) => Json(CommandResponse {
                stdout: "".to_string(),
                stderr: format!("Failed to access kernel: {}", e),
                status: Some(1),
                display_data: None,
            }),
        }
    }).await.unwrap_or_else(|e| Json(CommandResponse {
        stdout: "".to_string(),
        stderr: format!("Task join error: {}", e),
        status: Some(1),
        display_data: None,
    }))
}

async fn execute_cpp(code: String, context: Option<Vec<String>>) -> Json<CommandResponse> {
    tokio::task::spawn_blocking(move || {
        use crate::server::kernel::{self, Kernel};
        match kernel::get_or_init_cpp_kernel() {
            Ok(mut kernel_guard) => {
                if let Some(kernel) = kernel_guard.as_mut() {
                    match kernel.execute(code, None, context, None) {
                        Ok(response) => Json(CommandResponse {
                            stdout: response.stdout,
                            stderr: response.stderr,
                            status: response.status,
                            display_data: response.display_data,
                        }),
                        Err(e) => Json(CommandResponse {
                            stdout: "".to_string(),
                            stderr: format!("Kernel execution failed: {}", e),
                            status: Some(1),
                            display_data: None,
                        }),
                    }
                } else {
                    Json(CommandResponse {
                        stdout: "".to_string(),
                        stderr: "Kernel not initialized".to_string(),
                        status: Some(1),
                        display_data: None,
                    })
                }
            }
            Err(e) => Json(CommandResponse {
                stdout: "".to_string(),
                stderr: format!("Failed to access kernel: {}", e),
                status: Some(1),
                display_data: None,
            }),
        }
    }).await.unwrap_or_else(|e| {
        Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Task join error: {}", e),
            status: Some(1),
            display_data: None,
        })
    })
}

async fn execute_go(code: String, context: Option<Vec<String>>) -> Json<CommandResponse> {
    tokio::task::spawn_blocking(move || {
        use crate::server::kernel::{self, Kernel};
        match kernel::get_or_init_go_kernel() {
            Ok(mut kernel_guard) => {
                if let Some(kernel) = kernel_guard.as_mut() {
                    match kernel.execute(code, None, context, None) {
                        Ok(response) => Json(CommandResponse {
                            stdout: response.stdout,
                            stderr: response.stderr,
                            status: response.status,
                            display_data: response.display_data,
                        }),
                        Err(e) => Json(CommandResponse {
                            stdout: "".to_string(),
                            stderr: format!("Kernel execution failed: {}", e),
                            status: Some(1),
                            display_data: None,
                        }),
                    }
                } else {
                    Json(CommandResponse {
                        stdout: "".to_string(),
                        stderr: "Kernel not initialized".to_string(),
                        status: Some(1),
                        display_data: None,
                    })
                }
            }
            Err(e) => Json(CommandResponse {
                stdout: "".to_string(),
                stderr: format!("Failed to access kernel: {}", e),
                status: Some(1),
                display_data: None,
            }),
        }
    }).await.unwrap_or_else(|e| Json(CommandResponse {
        stdout: "".to_string(),
        stderr: format!("Task join error: {}", e),
        status: Some(1),
        display_data: None,
    }))
}

async fn log_requests(req: Request, next: Next) -> Response {
    next.run(req).await
}

pub fn app() -> Router {
    Router::new()
        .route("/exec", post(execute_command))
        .route("/export", post(export_notebook))
        .route("/files", post(list_files))
        .route("/files/read", post(read_file))
        .route("/files/save", post(save_file))
        .route("/files/rename", post(rename_file))
        .route("/files/copy", post(copy_file))
        .route("/files/delete", post(delete_file))
        .route("/config", get(get_config).post(update_config))
        .route("/input/status", get(check_input_status))
        .route("/input/submit", post(submit_input))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        )
        .layer(middleware::from_fn(log_requests))
}

async fn check_input_status() -> Json<InputStatusResponse> {
    let temp_dir = std::env::temp_dir();
    let req_path = temp_dir.join("newt_web_input_req");
    let res_path = temp_dir.join("newt_web_input_res");
    
    // If response already exists, we are waiting for kernel to pick it up, so don't prompt again
    if req_path.exists() && !res_path.exists() {
        let prompt = std::fs::read_to_string(req_path).unwrap_or_default();
        Json(InputStatusResponse { required: true, prompt })
    } else {
        Json(InputStatusResponse { required: false, prompt: String::new() })
    }
}

async fn submit_input(Json(payload): Json<InputSubmitRequest>) -> Json<String> {
    let temp_dir = std::env::temp_dir();
    let res_path = temp_dir.join("newt_web_input_res");
    std::fs::write(res_path, payload.value).unwrap_or_default();
    Json("OK".to_string())
}

pub async fn run_server() {
    let app = app();
    // Try to bind to the port, if it fails, assume server is already running and just return
    if let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:3030").await {
        // println!("Server running on http://127.0.0.1:3030");
        axum::serve(listener, app).await.unwrap();
    } else {
        eprintln!("Server already running on http://127.0.0.1:3030");
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Config {
    pub theme: Option<String>,
    pub editor: Option<String>,
    pub display_mode: Option<String>,
    pub colorscheme: Option<String>,
    pub show_hidden_files: Option<bool>,
    pub respect_gitignore: Option<bool>,
    pub savequit: Option<bool>,
}

pub async fn get_config() -> Json<Config> {
    let mut path = get_app_dir();
    path.push("config.json");
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(config) = serde_json::from_str(&content) {
            return Json(config);
        }
    }
    Json(Config::default())
}

pub async fn update_config(Json(config): Json<Config>) -> Json<String> {
    let mut path = get_app_dir();
    path.push("config.json");
    
    // Merge with existing config if possible
    let mut final_config = config;
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(existing) = serde_json::from_str::<Config>(&content) {
            if final_config.theme.is_none() {
                final_config.theme = existing.theme;
            }
            if final_config.editor.is_none() {
                final_config.editor = existing.editor;
            }
            if final_config.display_mode.is_none() {
                final_config.display_mode = existing.display_mode;
            }
            if final_config.colorscheme.is_none() {
                final_config.colorscheme = existing.colorscheme;
            }
            if final_config.show_hidden_files.is_none() {
                final_config.show_hidden_files = existing.show_hidden_files;
            }
            if final_config.respect_gitignore.is_none() {
                final_config.respect_gitignore = existing.respect_gitignore;
            }
            if final_config.savequit.is_none() {
                final_config.savequit = existing.savequit;
            }
        }
    }

    if let Ok(json) = serde_json::to_string_pretty(&final_config) {
        if fs::write(path, json).is_ok() {
            return Json("OK".to_string());
        }
    }
    Json("Error saving config".to_string())
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FileItem {
    pub path: Option<String>,
    pub label: String,
    pub is_header: bool,
    pub is_app_file: bool,
    pub is_directory: bool,
    pub is_expanded: bool,
    pub depth: usize,
}

#[derive(Deserialize)]
pub struct ListFilesRequest {
    #[serde(default)]
    expanded_dirs: Vec<String>,
}

pub async fn list_files(Json(req): Json<ListFilesRequest>) -> Json<Vec<FileItem>> {
    let mut files = Vec::new();

    let mut config_path = get_app_dir();
    config_path.push("config.json");
    let config = if let Ok(content) = fs::read_to_string(&config_path) {
        serde_json::from_str::<Config>(&content).unwrap_or_default()
    } else {
        Config::default()
    };
    let show_hidden_files = config.show_hidden_files.unwrap_or(false);
    let respect_gitignore = config.respect_gitignore.unwrap_or(false);

    // Convert expanded directories to a HashSet of PathBufs
    let expanded_dirs: std::collections::HashSet<PathBuf> = req
        .expanded_dirs
        .iter()
        .map(|s| PathBuf::from(s))
        .collect();

    // Local Files Header
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cwd_label = cwd
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| cwd.to_string_lossy().to_string());

    files.push(FileItem {
        path: None,
        label: cwd_label,
        is_header: true,
        is_app_file: false,
        is_directory: false,
        is_expanded: false,
        depth: 0,
    });

    // Build hierarchical tree for local files
    let local_gitignore = if respect_gitignore {
        Some(Gitignore::new(cwd.join(".gitignore")).0)
    } else {
        None
    };
    build_directory_tree(
        &cwd,
        0,
        false,
        &mut files,
        &expanded_dirs,
        show_hidden_files,
        respect_gitignore,
        local_gitignore.as_ref(),
    );

    // App Files Header
    files.push(FileItem {
        path: None,
        label: "Application Files".to_string(),
        is_header: true,
        is_app_file: true,
        is_directory: false,
        is_expanded: false,
        depth: 0,
    });

    // Build hierarchical tree for app files
    let app_dir = get_app_dir();
    build_directory_tree(
        &app_dir,
        0,
        true,
        &mut files,
        &expanded_dirs,
        show_hidden_files,
        false,
        None,
    );

    Json(files)
}

fn build_directory_tree(
    dir: &PathBuf,
    depth: usize,
    is_app_file: bool,
    files: &mut Vec<FileItem>,
    expanded_dirs: &std::collections::HashSet<PathBuf>,
    show_hidden_files: bool,
    respect_gitignore: bool,
    gitignore: Option<&Gitignore>,
) {
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
            let file_name = path.file_name().map(|n| n.to_string_lossy().to_string());
            let is_newt_dir = file_name.as_deref() == Some(".newt");

            if !show_hidden_files && !is_newt_dir {
                if let Some(name) = &file_name {
                    if name.starts_with('.') {
                        continue;
                    }
                }
            }

            if !is_app_file && respect_gitignore && !is_newt_dir {
                if let Some(matcher) = gitignore {
                    if matcher.matched_path_or_any_parents(&path, is_dir).is_ignore() {
                        continue;
                    }
                }
            }

            // For app files, only show .newt and .md files (but always show directories)
            if is_app_file && !is_dir {
                if !path.extension().map_or(false, |ext| ext == "newt" || ext == "md") {
                    continue;
                }
            }

            let label = path.file_name().unwrap().to_string_lossy().to_string();
            let is_expanded = expanded_dirs.contains(&path);

            files.push(FileItem {
                path: Some(path.to_string_lossy().to_string()),
                label,
                is_header: false,
                is_app_file,
                is_directory: is_dir,
                is_expanded,
                depth,
            });

            // Recursively add children if directory is expanded
            if is_dir && is_expanded {
                build_directory_tree(
                    &path,
                    depth + 1,
                    is_app_file,
                    files,
                    expanded_dirs,
                    show_hidden_files,
                    respect_gitignore,
                    gitignore,
                );
            }
        }
    }
}

#[derive(Deserialize)]
pub struct FilePath {
    path: String,
}

#[derive(Deserialize)]
pub struct SaveRequest {
    path: String,
    content: String,
}

#[derive(Deserialize)]
pub struct RenameRequest {
    old_path: String,
    new_path: String,
}

#[derive(Deserialize)]
pub struct CopyRequest {
    src: String,
    dest: String,
}

pub async fn read_file(Json(req): Json<FilePath>) -> Json<String> {
    // If path is absolute or starts from current directory, use it directly
    // Otherwise, treat it as relative to app directory (for backwards compatibility)
    let path = PathBuf::from(&req.path);
    let final_path = if path.is_absolute() {
        path
    } else {
        // Check if it exists in current directory first
        let cwd_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(&path);
        if cwd_path.exists() {
            cwd_path
        } else {
            // Fall back to app directory
            get_app_dir().join(&path)
        }
    };

    match fs::read_to_string(final_path) {
        Ok(content) => Json(content),
        Err(_) => Json("".to_string()),
    }
}

pub async fn save_file(Json(req): Json<SaveRequest>) -> Json<String> {
    // If path is absolute or starts from current directory, use it directly
    // Otherwise, treat it as relative to app directory (for backwards compatibility)
    let path = PathBuf::from(&req.path);
    let final_path = if path.is_absolute() {
        path
    } else {
        // Check if it exists in current directory first, or if parent exists in cwd
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let cwd_path = cwd.join(&path);
        if cwd_path.exists() || (cwd_path.parent().map_or(false, |p| p.exists())) {
            cwd_path
        } else {
            // Fall back to app directory
            get_app_dir().join(&path)
        }
    };

    match fs::write(final_path, req.content) {
        Ok(_) => Json("OK".to_string()),
        Err(e) => Json(format!("Error: {}", e)),
    }
}

pub async fn rename_file(Json(req): Json<RenameRequest>) -> Json<String> {
    let mut old_path = get_app_dir();
    old_path.push(req.old_path);
    let mut new_path = get_app_dir();
    new_path.push(req.new_path);
    match fs::rename(old_path, new_path) {
        Ok(_) => Json("OK".to_string()),
        Err(e) => Json(format!("Error: {}", e)),
    }
}

pub async fn copy_file(Json(req): Json<CopyRequest>) -> Json<String> {
    let mut src = get_app_dir();
    src.push(req.src);
    let mut dest = get_app_dir();
    dest.push(req.dest);
    match fs::copy(src, dest) {
        Ok(_) => Json("OK".to_string()),
        Err(e) => Json(format!("Error: {}", e)),
    }
}

pub async fn delete_file(Json(req): Json<FilePath>) -> Json<String> {
    let mut path = get_app_dir();
    path.push(req.path);
    if path.is_dir() {
        match fs::remove_dir_all(path) {
            Ok(_) => Json("OK".to_string()),
            Err(e) => Json(format!("Error: {}", e)),
        }
    } else {
        match fs::remove_file(path) {
            Ok(_) => Json("OK".to_string()),
            Err(e) => Json(format!("Error: {}", e)),
        }
    }
}
