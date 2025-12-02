use axum::{
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::process::Command;
use tower_http::cors::CorsLayer;
use regex::Regex;
use std::collections::HashSet;

#[derive(Deserialize, Serialize)]
pub struct CommandRequest {
    pub command: String,
    pub language: Option<String>,
    pub context: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommandResponse {
    pub stdout: String,
    pub stderr: String,
    pub status: Option<i32>,
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
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Cell {
    pub id: String,
    pub content: String,
    pub output: String,
    pub cell_type: CellType,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Notebook {
    pub cells: Vec<Cell>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExportResponse {
    pub markdown: String,
}

pub async fn execute_command(Json(payload): Json<CommandRequest>) -> Json<CommandResponse> {
    let language = payload.language.as_deref().unwrap_or("shell");

    match language {
        "rust" => execute_rust(payload.command, payload.context),
        "python" => execute_python(payload.command, payload.context),
        "javascript" => execute_javascript(payload.command, payload.context),
        "typescript" => execute_typescript(payload.command, payload.context),
        "c" => execute_c(payload.command, payload.context),
        "cpp" => execute_cpp(payload.command, payload.context),
        "go" => execute_go(payload.command, payload.context),
        _ => execute_shell(payload.command),
    }
}

pub async fn export_notebook(Json(notebook): Json<Notebook>) -> Json<ExportResponse> {
    let mut markdown = String::from("# Newt Notebook Export\n\n");

    for (i, cell) in notebook.cells.iter().enumerate() {
        let lang_str = match cell.cell_type {
            CellType::Shell => "bash",
            CellType::Rust => "rust",
            CellType::Python => "python",
            CellType::JavaScript => "javascript",
            CellType::TypeScript => "typescript",
            CellType::C => "c",
            CellType::Cpp => "cpp",
            CellType::Go => "go",
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

fn execute_shell(command: String) -> Json<CommandResponse> {
    let parts: Vec<&str> = command.split_whitespace().collect();
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

fn execute_rust(code: String, context: Option<String>) -> Json<CommandResponse> {
    let context_str = context.unwrap_or_default();
    // Simple regex to find 'fn main' anywhere
    let re_context_main = Regex::new(r"fn\s+main").unwrap();
    
    let source_code = if re_context_main.is_match(&context_str) {
        // Context has main. Treat as "New Program" sequence.
        // Rename old main to avoid conflict.
        let processed_context = re_context_main.replace_all(&context_str, "fn main_ignored").to_string();
        
        // Check if current code has main
        let re_code_main = Regex::new(r"fn\s+main").unwrap();
        let final_code = if re_code_main.is_match(&code) {
            code
        } else {
            format!("fn main() {{\n{}\n}}", code)
        };
        
        format!("{}\n{}", processed_context, final_code)
    } else {
        // Context has NO main. Treat as "Script" sequence.
        let full_code = format!("{}\n{}", context_str, code);
        let re_full_main = Regex::new(r"fn\s+main").unwrap();
        
        if re_full_main.is_match(&full_code) {
            // If full code has main, we assume it's a valid program.
            // BUT, if the main comes from context (which we missed above??) or code,
            // and there are statements outside, it will fail.
            // Since we are here, context definitely didn't have main (according to re_context_main).
            // So main must be in 'code'.
            // If 'code' has main, we trust the user.
            full_code
        } else {
            format!("fn main() {{\n{}\n}}", full_code)
        }
    };
    
    let temp_dir = std::env::temp_dir();
    let file_name = format!("newt_script_{}.rs", uuid::Uuid::new_v4());
    let file_path = temp_dir.join(&file_name);
    let bin_path = temp_dir.join(format!("newt_script_{}", uuid::Uuid::new_v4()));

    println!("DEBUG RUST SOURCE:\n{}", source_code);

    if let Err(e) = std::fs::write(&file_path, &source_code) {
         return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to write source file: {}", e),
            status: None,
        });
    }

    let compile_output = Command::new("rustc")
        .arg(&file_path)
        .arg("-o")
        .arg(&bin_path)
        .output();

    match compile_output {
        Ok(output) => {
            if !output.status.success() {
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

    let run_output = Command::new(&bin_path).output();

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

fn execute_python(code: String, context: Option<String>) -> Json<CommandResponse> {
    let full_code = if let Some(ctx) = context {
        format!("{}\n{}", ctx, code)
    } else {
        code
    };

    // Extract imports
    let re = Regex::new(r"(?m)^(?:import|from)\s+([a-zA-Z0-9_]+)").unwrap();
    let mut packages = HashSet::new();
    for cap in re.captures_iter(&full_code) {
        if let Some(pkg) = cap.get(1) {
            packages.insert(pkg.as_str().to_string());
        }
    }

    let temp_dir = std::env::temp_dir();
    let file_name = format!("newt_script_{}.py", uuid::Uuid::new_v4());
    let file_path = temp_dir.join(&file_name);

    if let Err(e) = std::fs::write(&file_path, &full_code) {
         return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to write source file: {}", e),
            status: None,
        });
    }

    let mut cmd = Command::new("uv");
    cmd.arg("run");
    
    for pkg in packages {
        let stdlib = ["os", "sys", "math", "json", "re", "time", "datetime", "random", "collections", "itertools", "functools"];
        if !stdlib.contains(&pkg.as_str()) {
            cmd.arg("--with").arg(pkg);
        }
    }
    
    cmd.arg(&file_path);

    let output = cmd.output();
    let _ = std::fs::remove_file(&file_path);

    match output {
        Ok(output) => Json(CommandResponse {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status: output.status.code(),
        }),
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to execute uv: {}", e),
            status: None,
        }),
    }
}

fn execute_javascript(code: String, context: Option<String>) -> Json<CommandResponse> {
    let full_code = if let Some(ctx) = context {
        format!("{}\n{}", ctx, code)
    } else {
        code
    };

    let temp_dir = std::env::temp_dir();
    let file_name = format!("newt_script_{}.js", uuid::Uuid::new_v4());
    let file_path = temp_dir.join(&file_name);

    if let Err(e) = std::fs::write(&file_path, &full_code) {
         return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to write source file: {}", e),
            status: None,
        });
    }

    let output = Command::new("node")
        .arg(&file_path)
        .output();
        
    let _ = std::fs::remove_file(&file_path);

    match output {
        Ok(output) => Json(CommandResponse {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status: output.status.code(),
        }),
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to execute node: {}", e),
            status: None,
        }),
    }
}

fn execute_typescript(code: String, context: Option<String>) -> Json<CommandResponse> {
    let full_code = if let Some(ctx) = context {
        format!("{}\n{}", ctx, code)
    } else {
        code
    };

    let temp_dir = std::env::temp_dir();
    let file_name = format!("newt_script_{}.ts", uuid::Uuid::new_v4());
    let file_path = temp_dir.join(&file_name);

    if let Err(e) = std::fs::write(&file_path, &full_code) {
         return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to write source file: {}", e),
            status: None,
        });
    }

    // Try npx tsx first
    let output = Command::new("npx")
        .arg("tsx")
        .arg(&file_path)
        .output();
        
    let _ = std::fs::remove_file(&file_path);

    match output {
        Ok(output) => Json(CommandResponse {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status: output.status.code(),
        }),
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to execute npx tsx: {}", e),
            status: None,
        }),
    }
}

fn execute_c(code: String, context: Option<String>) -> Json<CommandResponse> {
    let full_code = if let Some(ctx) = context {
        format!("{}\n{}", ctx, code)
    } else {
        code
    };

    let temp_dir = std::env::temp_dir();
    let file_name = format!("newt_script_{}.c", uuid::Uuid::new_v4());
    let file_path = temp_dir.join(&file_name);
    let bin_path = temp_dir.join(format!("newt_script_{}", uuid::Uuid::new_v4()));

    if let Err(e) = std::fs::write(&file_path, &full_code) {
         return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to write source file: {}", e),
            status: None,
        });
    }

    let compile_output = Command::new("gcc")
        .arg(&file_path)
        .arg("-o")
        .arg(&bin_path)
        .output();

    match compile_output {
        Ok(output) => {
            if !output.status.success() {
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
                stderr: format!("Failed to execute gcc: {}", e),
                status: None,
            });
        }
    }

    let run_output = Command::new(&bin_path).output();

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

fn execute_cpp(code: String, context: Option<String>) -> Json<CommandResponse> {
    let context_str = context.unwrap_or_default();
    let re_context_main = Regex::new(r"(?m)int\s+main\s*\(").unwrap();
    
    let mut source_code = if re_context_main.is_match(&context_str) {
        // Context has main. Rename it.
        let processed_context = re_context_main.replace_all(&context_str, "int main_ignored(").to_string();
        
        let re_code_main = Regex::new(r"(?m)int\s+main\s*\(").unwrap();
        let final_code = if re_code_main.is_match(&code) {
            code
        } else {
            format!("int main() {{\n{}\nreturn 0;\n}}", code)
        };
        
        format!("{}\n{}", processed_context, final_code)
    } else {
        // Context has NO main.
        let full_code = format!("{}\n{}", context_str, code);
        let re_full_main = Regex::new(r"(?m)int\s+main\s*\(").unwrap();
        
        if re_full_main.is_match(&full_code) {
            full_code
        } else {
            format!("int main() {{\n{}\nreturn 0;\n}}", full_code)
        }
    };

    // Ensure includes
    if !source_code.contains("#include <iostream>") {
        source_code = format!("#include <iostream>\n{}", source_code);
    }

    let temp_dir = std::env::temp_dir();
    let file_name = format!("newt_script_{}.cpp", uuid::Uuid::new_v4());
    let file_path = temp_dir.join(&file_name);
    let bin_path = temp_dir.join(format!("newt_script_{}", uuid::Uuid::new_v4()));

    if let Err(e) = std::fs::write(&file_path, &source_code) {
         return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to write source file: {}", e),
            status: None,
        });
    }

    let compile_output = Command::new("clang++")
        .arg(&file_path)
        .arg("-I/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/c++/v1")
        .arg("-o")
        .arg(&bin_path)
        .output();

    match compile_output {
        Ok(output) => {
            if !output.status.success() {
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
                stderr: format!("Failed to execute g++: {}", e),
                status: None,
            });
        }
    }

    let run_output = Command::new(&bin_path).output();

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

fn execute_go(code: String, context: Option<String>) -> Json<CommandResponse> {
    let full_code = if let Some(ctx) = context {
        format!("{}\n{}", ctx, code)
    } else {
        code
    };

    let temp_dir = std::env::temp_dir();
    let file_name = format!("newt_script_{}.go", uuid::Uuid::new_v4());
    let file_path = temp_dir.join(&file_name);

    if let Err(e) = std::fs::write(&file_path, &full_code) {
         return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to write source file: {}", e),
            status: None,
        });
    }

    let output = Command::new("go")
        .arg("run")
        .arg(&file_path)
        .output();
        
    let _ = std::fs::remove_file(&file_path);

    match output {
        Ok(output) => Json(CommandResponse {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status: output.status.code(),
        }),
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to execute go run: {}", e),
            status: None,
        }),
    }
}

pub fn app() -> Router {
    Router::new()
        .route("/exec", post(execute_command))
        .route("/export", post(export_notebook))
        .layer(CorsLayer::permissive())
}
