use axum::{
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::io::Write;
use tempfile::NamedTempFile;
use tower_http::cors::CorsLayer;

#[derive(Deserialize, Serialize)]
pub struct CommandRequest {
    pub command: String,
    pub language: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommandResponse {
    pub stdout: String,
    pub stderr: String,
    pub status: Option<i32>,
}

pub async fn execute_command(Json(payload): Json<CommandRequest>) -> Json<CommandResponse> {
    let language = payload.language.as_deref().unwrap_or("shell");

    if language == "rust" {
        return execute_rust(payload.command);
    }

    // Security warning: This is extremely dangerous in a real environment.
    // It allows arbitrary command execution.
    // For this demo, we assume it's a local tool for the user.
    
    // We'll split the command string into program and args roughly.
    // A real shell would handle parsing better.
    let parts: Vec<&str> = payload.command.split_whitespace().collect();
    if parts.is_empty() {
        return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: "Empty command".to_string(),
            status: None,
        });
    }

    let program = parts[0];
    let args = &parts[1..];

    let output = Command::new(program)
        .args(args)
        .output();

    match output {
        Ok(output) => Json(CommandResponse {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status: output.status.code(),
        }),
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: e.to_string(),
            status: None,
        }),
    }
}

fn execute_rust(code: String) -> Json<CommandResponse> {
    // Create a temporary file for the Rust source code
    let mut src_file = match NamedTempFile::new() {
        Ok(f) => f,
        Err(e) => return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to create temp file: {}", e),
            status: None,
        }),
    };

    // Wrap the code in a main function
    let source_code = format!("fn main() {{\n{}\n}}", code);
    
    if let Err(e) = write!(src_file, "{}", source_code) {
        return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to write to temp file: {}", e),
            status: None,
        });
    }

    // Keep the file path but persist it? No, NamedTempFile deletes on drop.
    // We need to persist it long enough to compile.
    // Actually, rustc needs a path with .rs extension usually, or we can pass via stdin?
    // rustc accepts input from stdin with `-`.
    // But let's try to use a file with .rs extension.
    
    let temp_dir = std::env::temp_dir();
    let file_name = format!("newt_script_{}.rs", uuid::Uuid::new_v4());
    let file_path = temp_dir.join(&file_name);
    let bin_path = temp_dir.join(format!("newt_script_{}", uuid::Uuid::new_v4()));

    if let Err(e) = std::fs::write(&file_path, source_code) {
         return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to write source file: {}", e),
            status: None,
        });
    }

    // Compile
    let compile_output = Command::new("rustc")
        .arg(&file_path)
        .arg("-o")
        .arg(&bin_path)
        .output();

    match compile_output {
        Ok(output) => {
            if !output.status.success() {
                // Cleanup source
                let _ = std::fs::remove_file(&file_path);
                return Json(CommandResponse {
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    status: output.status.code(),
                });
            }
        }
        Err(e) => {
            let _ = std::fs::remove_file(&file_path);
            return Json(CommandResponse {
                stdout: "".to_string(),
                stderr: format!("Failed to execute rustc: {}", e),
                status: None,
            });
        }
    }

    // Run
    let run_output = Command::new(&bin_path).output();

    // Cleanup
    let _ = std::fs::remove_file(&file_path);
    let _ = std::fs::remove_file(&bin_path);

    match run_output {
        Ok(output) => Json(CommandResponse {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status: output.status.code(),
        }),
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to run compiled binary: {}", e),
            status: None,
        }),
    }
}

pub fn app() -> Router {
    Router::new()
        .route("/exec", post(execute_command))
        .layer(CorsLayer::permissive())
}
