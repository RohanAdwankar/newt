use axum::{
    Json, Router,
    routing::{get, post},
};
use directories::ProjectDirs;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tower_http::cors::CorsLayer;

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
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Cell {
    pub id: String,
    pub content: String,
    pub output: String,
    pub cell_type: CellType,
    #[serde(default)]
    pub polling_interval: Option<u64>, // Interval in seconds
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
            display_data: None,
        });
    }

    let program = parts[0];
    let args = &parts[1..];

    let output = Command::new(program).args(args).output();

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

fn execute_rust(code: String, context: Option<String>) -> Json<CommandResponse> {
    let context_str = context.unwrap_or_default();
    // Simple regex to find 'fn main' anywhere
    let re_context_main = Regex::new(r"fn\s+main").unwrap();

    let source_code = if re_context_main.is_match(&context_str) {
        // Context has main. Treat as "New Program" sequence.
        // Rename old main to avoid conflict.
        let processed_context = re_context_main
            .replace_all(&context_str, "fn main_ignored")
            .to_string();

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
            display_data: None,
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
                    display_data: None,
                });
            }
        }
        Err(e) => {
            let _ = std::fs::remove_file(&file_path);
            return Json(CommandResponse {
                stdout: "".to_string(),
                stderr: format!("Failed to execute rustc: {}", e),
                status: None,
                display_data: None,
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
            display_data: None,
        }),
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to run compiled binary: {}", e),
            status: None,
            display_data: None,
        }),
    }
}

fn execute_python(code: String, context: Option<String>) -> Json<CommandResponse> {
    let full_code = if let Some(ctx) = context {
        let indented_ctx = ctx
            .lines()
            .map(|line| format!("    {}", line))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            r#"
import sys
import os
_newt_old_stdout = sys.stdout
sys.stdout = open(os.devnull, 'w')
try:
{}
    pass
finally:
    sys.stdout.close()
    sys.stdout = _newt_old_stdout

{}
"#,
            indented_ctx, code
        )
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

    // Create a directory for outputs
    // Use a persistent directory in the standard data directory
    let images_dir = if let Some(proj_dirs) = ProjectDirs::from("com", "newt", "newt") {
        let data_dir = proj_dirs.data_dir();
        data_dir.join("images")
    } else {
        // Fallback to temp dir if we can't find a data dir
        std::env::temp_dir().join("newt_images")
    };

    if !images_dir.exists() {
        let _ = std::fs::create_dir_all(&images_dir);
    }

    let output_dir_name = format!("newt_output_{}", uuid::Uuid::new_v4());
    let output_dir = images_dir.join(&output_dir_name);
    if let Err(e) = std::fs::create_dir(&output_dir) {
        return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to create output directory: {}", e),
            status: None,
            display_data: None,
        });
    }
    let output_dir_str = output_dir.to_string_lossy().replace("\\", "\\\\");

    // Python wrapper script
    let wrapper_script = format!(
        r#"
import sys
import os
import json
import base64
import builtins

_newt_outputs = []
_newt_output_dir = "{}"

def _newt_display(obj, **kwargs):
    data = {{}}
    metadata = {{}}
    
    if hasattr(obj, "_repr_mimebundle_"):
        try:
            data, metadata = obj._repr_mimebundle_(include=None, exclude=None)
        except Exception:
            pass
            
    if not data:
        if hasattr(obj, "_repr_png_"):
            data["image/png"] = obj._repr_png_()
        if hasattr(obj, "_repr_svg_"):
            data["image/svg+xml"] = obj._repr_svg_()
        if hasattr(obj, "_repr_html_"):
            data["text/html"] = obj._repr_html_()
        if hasattr(obj, "_repr_json_"):
            data["application/json"] = obj._repr_json_()
        if hasattr(obj, "__repr__"):
            data["text/plain"] = obj.__repr__()

    if data:
        processed_data = {{}}
        for mime, content in data.items():
            if mime.startswith("image/"):
                if isinstance(content, bytes):
                    b64 = base64.b64encode(content).decode('utf-8')
                    processed_data[mime] = b64
                else:
                    processed_data[mime] = content
            else:
                processed_data[mime] = content
        
        _newt_outputs.append({{
            "data": processed_data,
            "metadata": metadata
        }})

builtins.display = _newt_display

# --- User Code ---
{}
# -----------------

if "matplotlib.pyplot" in sys.modules:
    try:
        import matplotlib.pyplot as plt
        # Check for open figures
        for i in plt.get_fignums():
            fig = plt.figure(i)
            from io import BytesIO
            buf = BytesIO()
            fig.savefig(buf, format='png')
            b64 = base64.b64encode(buf.getvalue()).decode('utf-8')
            
            _newt_outputs.append({{
                "data": {{"image/png": b64}},
                "metadata": {{}}
            }})
            plt.close(i)
    except Exception:
        pass

# Trigger post-execute events (standard Jupyter lifecycle)

if _newt_outputs:
    print(f"\n__NEWT_DISPLAY_START__")
    print(json.dumps(_newt_outputs))
    print(f"__NEWT_DISPLAY_END__")

"#,
        output_dir_str, full_code
    );

    if let Err(e) = std::fs::write(&file_path, &wrapper_script) {
        return Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to write source file: {}", e),
            status: None,
            display_data: None,
        });
    }

    let mut cmd = Command::new("uv");
    cmd.arg("run");

    // Force Agg backend to avoid popups
    cmd.env("MPLBACKEND", "Agg");

    for pkg in packages {
        let stdlib = [
            "os",
            "sys",
            "math",
            "json",
            "re",
            "time",
            "datetime",
            "random",
            "collections",
            "itertools",
            "functools",
        ];
        if !stdlib.contains(&pkg.as_str()) {
            cmd.arg("--with").arg(pkg);
        }
    }

    cmd.arg(&file_path);

    let output = cmd.output();
    let _ = std::fs::remove_file(&file_path);

    match output {
        Ok(output) => {
            let stdout_raw = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            let mut stdout = stdout_raw.clone();
            let mut display_data = None;

            if let Some(start_idx) = stdout_raw.find("__NEWT_DISPLAY_START__") {
                if let Some(end_idx) = stdout_raw.find("__NEWT_DISPLAY_END__") {
                    let json_str =
                        &stdout_raw[start_idx + "__NEWT_DISPLAY_START__".len()..end_idx].trim();
                    if let Ok(data) = serde_json::from_str::<Vec<DisplayData>>(json_str) {
                        for item in &data {
                            println!("Generated display data with keys: {:?}", item.data.keys());
                        }
                        display_data = Some(data);
                    }
                    // Remove the display block from stdout
                    stdout = format!(
                        "{}{}",
                        &stdout_raw[..start_idx],
                        &stdout_raw[end_idx + "__NEWT_DISPLAY_END__".len()..]
                    );
                }
            }

            Json(CommandResponse {
                stdout: stdout.trim().to_string(),
                stderr,
                status: output.status.code(),
                display_data,
            })
        }
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to execute uv: {}", e),
            status: None,
            display_data: None,
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
            display_data: None,
        });
    }

    let output = Command::new("node").arg(&file_path).output();

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
            stderr: format!("Failed to execute node: {}", e),
            status: None,
            display_data: None,
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
            display_data: None,
        });
    }

    // Try npx tsx first
    let output = Command::new("npx").arg("tsx").arg(&file_path).output();

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
            display_data: None,
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
                    display_data: None,
                });
            }
        }
        Err(e) => {
            let _ = std::fs::remove_file(&file_path);
            return Json(CommandResponse {
                stdout: "".to_string(),
                stderr: format!("Failed to execute gcc: {}", e),
                status: None,
                display_data: None,
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
            display_data: None,
        }),
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to run compiled binary: {}", e),
            status: None,
            display_data: None,
        }),
    }
}

fn execute_cpp(code: String, context: Option<String>) -> Json<CommandResponse> {
    let context_str = context.unwrap_or_default();
    let re_context_main = Regex::new(r"(?m)int\s+main\s*\(").unwrap();

    let mut source_code = if re_context_main.is_match(&context_str) {
        // Context has main. Rename it.
        let processed_context = re_context_main
            .replace_all(&context_str, "int main_ignored(")
            .to_string();

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
            display_data: None,
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
                    display_data: None,
                });
            }
        }
        Err(e) => {
            let _ = std::fs::remove_file(&file_path);
            return Json(CommandResponse {
                stdout: "".to_string(),
                stderr: format!("Failed to execute g++: {}", e),
                status: None,
                display_data: None,
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
            display_data: None,
        }),
        Err(e) => Json(CommandResponse {
            stdout: "".to_string(),
            stderr: format!("Failed to run compiled binary: {}", e),
            status: None,
            display_data: None,
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
            display_data: None,
        });
    }

    let output = Command::new("go").arg("run").arg(&file_path).output();

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
            stderr: format!("Failed to execute go run: {}", e),
            status: None,
            display_data: None,
        }),
    }
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
        .route("/config", get(get_config).post(update_config))
        .layer(CorsLayer::permissive())
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Config {
    pub theme: Option<String>,
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
