use axum::{
    Json, Router,
    routing::{get, post},
    extract::Request,
    middleware::{self, Next},
    response::Response,
};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub mod kernel;
use tower_http::cors::{CorsLayer, Any};

use crate::{CommandRequest, CommandResponse, CellType, Notebook, ExportResponse};

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
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: "Empty command".to_string(),
            status: None,
            display_data: None,
        });
    }

    let program = parts[0].to_string();
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    let output = tokio::process::Command::new(program).args(args).output().await;

    match output {
        Ok(output) => Json(CommandResponse {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status: output.status.code(),
            display_data: None,
        }),
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: e.to_string(),
            status: None,
            display_data: None,
        }),
    }
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
        .route("/files", get(list_files))
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
    pub accent_color: Option<u8>,
    pub display_mode: Option<String>,
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
            if final_config.accent_color.is_none() {
                final_config.accent_color = existing.accent_color;
            }
            if final_config.display_mode.is_none() {
                final_config.display_mode = existing.display_mode;
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

pub async fn list_files() -> Json<Vec<String>> {
    let dir = get_app_dir();
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "newt") {
                if let Some(name) = path.file_name() {
                    files.push(name.to_string_lossy().to_string());
                }
            }
        }
    }
    files.sort();
    Json(files)
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
    let mut path = get_app_dir();
    path.push(req.path);
    match fs::read_to_string(path) {
        Ok(content) => Json(content),
        Err(_) => Json("".to_string()),
    }
}

pub async fn save_file(Json(req): Json<SaveRequest>) -> Json<String> {
    let mut path = get_app_dir();
    path.push(req.path);
    match fs::write(path, req.content) {
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
