use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use directories::ProjectDirs;

#[derive(Serialize, Deserialize, Debug)]
pub struct KernelResponse {
    pub stdout: String,
    pub stderr: String,
    pub status: Option<i32>,
    pub display_data: Option<Vec<crate::DisplayData>>,
}

#[derive(Serialize)]
struct KernelRequest {
    code: String,
    language: Option<String>,
    client_type: Option<String>,
}

pub trait Kernel: Send + Sync {
    fn execute(&mut self, code: String, language: Option<String>, context: Option<Vec<String>>, client_type: Option<String>) -> Result<KernelResponse, String>;
}

pub struct PythonKernel {
    #[allow(dead_code)]
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Kernel for PythonKernel {
    fn execute(&mut self, code: String, _language: Option<String>, _context: Option<Vec<String>>, client_type: Option<String>) -> Result<KernelResponse, String> {
        let req = KernelRequest { code, language: None, client_type };
        let json_req = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        
        writeln!(self.stdin, "{}", json_req).map_err(|e| e.to_string())?;
        
        let mut line = String::new();
        self.stdout.read_line(&mut line).map_err(|e| e.to_string())?;
        
        if line.is_empty() {
            return Err("Kernel process closed unexpectedly".to_string());
        }

        let response: KernelResponse = serde_json::from_str(&line).map_err(|e| format!("Failed to parse kernel response: {} | Raw: {}", e, line))?;
        Ok(response)
    }
}

impl PythonKernel {
    pub fn new(notebook_path: Option<String>) -> Result<Self, String> {
        // Create a directory for outputs (images)
        let images_dir = if let Some(proj_dirs) = ProjectDirs::from("com", "newt", "newt") {
            let data_dir = proj_dirs.data_dir();
            data_dir.join("images")
        } else {
            std::env::temp_dir().join("newt_images")
        };

        if !images_dir.exists() {
            let _ = std::fs::create_dir_all(&images_dir);
        }
        let output_dir_str = images_dir.to_string_lossy().replace("\\", "\\\\");

        // The persistent wrapper script
        let script = format!(r#"
import sys
import os
import json
import base64
import builtins
import traceback
import io
import contextlib
import subprocess
import tempfile

_newt_output_dir = "{}"
_newt_globals = {{}}
_newt_client_type = None

# Ensure we can import modules from current directory
sys.path.append(os.getcwd())

def _newt_input(prompt=""):
    # File-based coordination for all clients (Web and TUI)
    import time
    import os
    import tempfile
    
    # Define paths (using temp dir)
    temp_dir = tempfile.gettempdir()
    req_path = os.path.join(temp_dir, "newt_web_input_req")
    res_path = os.path.join(temp_dir, "newt_web_input_res")
    
    # Clean up any stale response file
    if os.path.exists(res_path):
        try:
            os.remove(res_path)
        except:
            pass

    # Write request
    with open(req_path, "w") as f:
        f.write(prompt)
        
    # Wait for response
    start_time = time.time()
    while not os.path.exists(res_path):
        if time.time() - start_time > 300: # 5 min timeout
            break
        time.sleep(0.1)
        
    # Read response
    result = ""
    if os.path.exists(res_path):
        with open(res_path, "r") as f:
            result = f.read()
        
    # Cleanup
    try:
        if os.path.exists(req_path): os.remove(req_path)
        if os.path.exists(res_path): os.remove(res_path)
    except:
        pass
        
    return result

def _newt_process_display_data(obj):
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
        return {{ "data": processed_data, "metadata": metadata }}
    return None

def main():
    # Setup matplotlib backend if available
    try:
        import matplotlib
        matplotlib.use('Agg')
        import matplotlib.pyplot as plt
        import warnings
        warnings.filterwarnings("ignore", message=".*FigureCanvasAgg is non-interactive.*")
    except ImportError:
        plt = None

    while True:
        try:
            line = sys.stdin.readline()
            if not line:
                break
            
            req = json.loads(line)
            code = req.get('code', '')
            global _newt_client_type
            _newt_client_type = req.get('client_type')
            
            stdout_capture = io.StringIO()
            stderr_capture = io.StringIO()
            display_outputs = []

            # Custom display function for this execution
            def _display(obj, **kwargs):
                res = _newt_process_display_data(obj)
                if res:
                    display_outputs.append(res)
            
            # builtins.display is global. 
            builtins.display = _display
            builtins.input = _newt_input

            import ast
            success = True
            with contextlib.redirect_stdout(stdout_capture), contextlib.redirect_stderr(stderr_capture):
                try:
                    # Parse code to AST
                    tree = ast.parse(code)
                    last_expr = None
                    if tree.body and isinstance(tree.body[-1], ast.Expr):
                        last_expr = tree.body.pop()
                    
                    # Execute all but the last statement
                    if tree.body:
                        exec(compile(tree, filename="<string>", mode="exec"), _newt_globals)
                    
                    # Evaluate the last expression if it exists
                    if last_expr:
                        res = eval(compile(ast.Expression(last_expr.value), filename="<string>", mode="eval"), _newt_globals)
                        if res is not None:
                            # Print repr to stdout so it shows up in TUI and Frontend
                            print(repr(res))
                            # Also try to display rich content if available
                            _display(res)

                except Exception:
                    traceback.print_exc()
                    success = False

            # Handle Matplotlib figures
            if plt:
                for i in plt.get_fignums():
                    fig = plt.figure(i)
                    buf = io.BytesIO()
                    fig.savefig(buf, format='png')
                    b64 = base64.b64encode(buf.getvalue()).decode('utf-8')
                    display_outputs.append({{
                        "data": {{"image/png": b64}},
                        "metadata": {{}}
                    }})
                    plt.close(i)

            response = {{
                "stdout": stdout_capture.getvalue(),
                "stderr": stderr_capture.getvalue(),
                "status": 0 if success else 1,
                "display_data": display_outputs if display_outputs else None
            }}
            
            print(json.dumps(response))
            sys.stdout.flush()

        except Exception as e:
            # Fallback error handling if the loop crashes logic (shouldn't happen often)
            err_resp = {{
                "stdout": "",
                "stderr": f"Kernel Error: {{str(e)}}",
                "status": 1,
                "display_data": None
            }}
            print(json.dumps(err_resp))
            sys.stdout.flush()

if __name__ == "__main__":
    main()
"#, output_dir_str);

        // Write script to temp file
        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join(format!("newt_kernel_{}.py", uuid::Uuid::new_v4()));
        std::fs::write(&script_path, script).map_err(|e| e.to_string())?;

        // Spawn python process
        // Use "uv run python" which will automatically detect and use the local pyproject.toml
        let mut cmd = Command::new("uv");
        cmd.arg("run");
        cmd.arg("python");
        cmd.arg(&script_path);

        // Set the working directory to the notebook's directory so uv can find pyproject.toml
        // If no notebook path is available, fall back to current directory
        let working_dir = if let Some(ref nb_path) = notebook_path {
            std::path::Path::new(nb_path).parent().map(|p| p.to_path_buf())
        } else {
            None
        }.or_else(|| std::env::current_dir().ok());

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        // Avoid leaking resolver/debug output into the TUI while the server is running.
        cmd.stderr(Stdio::null());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn python kernel: {}", e))?;

        let stdin = child.stdin.take().ok_or("Failed to open stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("Failed to open stdout")?);

        Ok(PythonKernel {
            child,
            stdin,
            stdout,
        })
    }
}

// Global Kernel Manager
// For now, we just hold a single Python kernel.
// In the future, this would be a Map<SessionId, Kernel>
lazy_static::lazy_static! {
    pub static ref PYTHON_KERNEL: Arc<Mutex<Option<PythonKernel>>> = Arc::new(Mutex::new(None));
}

pub fn get_or_init_python_kernel(notebook_path: Option<String>) -> Result<std::sync::MutexGuard<'static, Option<PythonKernel>>, String> {
    let mut kernel_guard = PYTHON_KERNEL.lock().map_err(|_| "Failed to lock kernel mutex".to_string())?;

    if kernel_guard.is_none() {
        let kernel = PythonKernel::new(notebook_path)?;
        *kernel_guard = Some(kernel);
    }

    Ok(kernel_guard)
}

pub struct NodeKernel {
    #[allow(dead_code)]
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Kernel for NodeKernel {
    fn execute(&mut self, code: String, language: Option<String>, _context: Option<Vec<String>>, client_type: Option<String>) -> Result<KernelResponse, String> {
        let req = KernelRequest { code, language, client_type };
        let json_req = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        
        writeln!(self.stdin, "{}", json_req).map_err(|e| e.to_string())?;
        
        let mut line = String::new();
        self.stdout.read_line(&mut line).map_err(|e| e.to_string())?;
        
        if line.is_empty() {
            return Err("Kernel process closed unexpectedly".to_string());
        }

        let response: KernelResponse = serde_json::from_str(&line).map_err(|e| format!("Failed to parse kernel response: {} | Raw: {}", e, line))?;
        Ok(response)
    }
}

impl NodeKernel {
    pub fn new() -> Result<Self, String> {
        let script = r#"
const readline = require('readline');
const vm = require('vm');
const fs = require('fs');
const os = require('os');
const path = require('path');

let ts = null;
try {
    ts = require('typescript');
} catch (e) {
    // ignore
}

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: false
});

// Buffers for the current execution
let current_stdout = [];
let current_stderr = [];

// Custom console for the VM
const customConsole = {
    log: (...args) => {
        current_stdout.push(args.map(a => String(a)).join(' '));
    },
    error: (...args) => {
        current_stderr.push(args.map(a => String(a)).join(' '));
    },
    warn: (...args) => {
        current_stderr.push(args.map(a => String(a)).join(' '));
    }
};

function input(promptText) {
    promptText = promptText || "";
    const tempDir = os.tmpdir();
    const reqPath = path.join(tempDir, "newt_web_input_req");
    const resPath = path.join(tempDir, "newt_web_input_res");

    process.stderr.write(`NodeKernel: input called. Req: ${reqPath}\n`);

    // Clean up stale response
    if (fs.existsSync(resPath)) {
        try { fs.unlinkSync(resPath); } catch(e) {}
    }

    // Write request
    fs.writeFileSync(reqPath, promptText);
    process.stderr.write(`NodeKernel: wrote request\n`);

    // Wait for response
    const sab = new SharedArrayBuffer(4);
    const int32 = new Int32Array(sab);
    const startTime = Date.now();
    
    while (!fs.existsSync(resPath)) {
        if (Date.now() - startTime > 300000) { // 5 min timeout
            break;
        }
        // Sleep for 100ms
        Atomics.wait(int32, 0, 0, 100);
    }

    let result = "";
    if (fs.existsSync(resPath)) {
        result = fs.readFileSync(resPath, 'utf8');
    }

    // Cleanup
    try {
        if (fs.existsSync(reqPath)) fs.unlinkSync(reqPath);
        if (fs.existsSync(resPath)) fs.unlinkSync(resPath);
    } catch(e) {}

    return result;
}

const context = vm.createContext({
  console: customConsole,
  require: require,
  process: process,
  setTimeout: setTimeout,
  setInterval: setInterval,
  clearTimeout: clearTimeout,
  clearInterval: clearInterval,
  input: input,
  prompt: input
});

rl.on('line', (line) => {
    if (!line) return;
    try {
        const req = JSON.parse(line);
        let code = req.code;
        const language = req.language;
        
        current_stdout = [];
        current_stderr = [];
        
        try {
            if (language === 'typescript') {
                if (!ts) {
                    throw new Error("TypeScript dependency not found. Please install 'typescript' in your project.");
                }
                const result = ts.transpileModule(code, { 
                    compilerOptions: { module: ts.ModuleKind.CommonJS } 
                });
                code = result.outputText;
            }
            
            const result = vm.runInContext(code, context);
            // If result is interesting, maybe print it?
            // For now, we rely on explicit console.log from user
        } catch (e) {
            current_stderr.push(e.toString());
        }
        
        const response = {
            stdout: current_stdout.join('\n'),
            stderr: current_stderr.join('\n'),
            status: current_stderr.length > 0 ? 1 : 0,
            display_data: null
        };
        
        // Use process.stdout.write directly to avoid our custom console if it leaked (it shouldn't)
        process.stdout.write(JSON.stringify(response) + '\n');
    } catch (e) {
        // JSON parse error or system error
        const response = {
            stdout: "",
            stderr: "Kernel System Error: " + e.toString(),
            status: 1,
            display_data: null
        };
        process.stdout.write(JSON.stringify(response) + '\n');
    }
});
"#;
        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join(format!("newt_node_kernel_{}.js", uuid::Uuid::new_v4()));
        std::fs::write(&script_path, script).map_err(|e| e.to_string())?;

        let mut cmd = Command::new("node");
        cmd.arg(&script_path);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn node kernel: {}", e))?;
        let stdin = child.stdin.take().ok_or("Failed to open stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("Failed to open stdout")?);

        Ok(NodeKernel {
            child,
            stdin,
            stdout,
        })
    }
}

lazy_static::lazy_static! {
    pub static ref NODE_KERNEL: Arc<Mutex<Option<NodeKernel>>> = Arc::new(Mutex::new(None));
    pub static ref RUST_KERNEL: Arc<Mutex<Option<RustKernel>>> = Arc::new(Mutex::new(None));
    pub static ref C_KERNEL: Arc<Mutex<Option<CKernel>>> = Arc::new(Mutex::new(None));
    pub static ref CPP_KERNEL: Arc<Mutex<Option<CppKernel>>> = Arc::new(Mutex::new(None));
    pub static ref GO_KERNEL: Arc<Mutex<Option<GoKernel>>> = Arc::new(Mutex::new(None));
}

pub fn get_or_init_node_kernel() -> Result<std::sync::MutexGuard<'static, Option<NodeKernel>>, String> {
    let mut kernel_guard = NODE_KERNEL.lock().map_err(|_| "Failed to lock kernel mutex".to_string())?;
    
    if kernel_guard.is_none() {
        let kernel = NodeKernel::new()?;
        *kernel_guard = Some(kernel);
    }
    
    Ok(kernel_guard)
}

pub struct RustKernel;

impl Kernel for RustKernel {
    fn execute(&mut self, code: String, _language: Option<String>, context: Option<Vec<String>>, _client_type: Option<String>) -> Result<KernelResponse, String> {
        let mut full_history = context.unwrap_or_default();
        full_history.push(code);
        
        let source = self.construct_source(&full_history);
        
        self.compile_and_run(&source)
    }
}

impl RustKernel {
    pub fn new() -> Self {
        Self
    }

    pub fn clear(&mut self) {
        // Stateless
    }

    fn construct_source(&self, history: &[String]) -> String {
        let mut items = Vec::new();
        let mut stmts = Vec::new();
        let mut has_user_main = false;

        for (idx, block) in history.iter().enumerate() {
            let is_last = idx == history.len() - 1;
            let mut content = block.trim();

            // Extract leading 'use' statements to handle mixed blocks like "use std::io; let x = ..."
            while content.starts_with("use ") {
                if let Some(end) = content.find(';') {
                    let use_stmt = &content[..=end];
                    items.push(use_stmt);
                    content = content[end+1..].trim();
                } else {
                    break;
                }
            }

            if content.is_empty() {
                continue;
            }

            if content.starts_with("fn ") || content.starts_with("struct ") || content.starts_with("enum ") || content.starts_with("impl ") || content.starts_with("mod ") || content.starts_with("type ") {
                if content.starts_with("fn main") {
                    has_user_main = true;
                }
                items.push(content);
            } else {
                // Only include statements from the CURRENT cell (last cell in history)
                // This prevents re-execution of previous cells' code
                if is_last {
                    stmts.push(content);
                }
            }
        }
        
        if !stmts.is_empty() {
            // If we have statements, we must wrap them in a main function.
            // To avoid conflict, we filter out any user-provided main.
            let filtered_items: Vec<&str> = items.into_iter()
                .filter(|i| !i.trim().starts_with("fn main"))
                .collect();
                
            return format!(r#"
#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]
{}

// Newt Input Support
use std::io::Read;
use std::io::Write;
use std::thread;
use std::time::Duration;
use std::fs;
use std::env;

mod newt_io {{
    use std::io;
    pub struct Stdin;
    pub fn stdin() -> Stdin {{ Stdin }}
    impl Stdin {{
        pub fn read_line(&self, buf: &mut String) -> io::Result<usize> {{
            let content = super::input();
            buf.push_str(&content);
            if !buf.ends_with('\n') {{
                buf.push('\n');
            }}
            Ok(content.len() + 1)
        }}
    }}
}}

fn input() -> String {{
    let temp_dir = env::temp_dir();
    let req_path = temp_dir.join("newt_web_input_req");
    let res_path = temp_dir.join("newt_web_input_res");

    if res_path.exists() {{
        let _ = fs::remove_file(&res_path);
    }}

    // Write request
    if let Ok(_) = fs::write(&req_path, "") {{
        // Wait for response
        while !res_path.exists() {{
            thread::sleep(Duration::from_millis(100));
        }}
        
        if let Ok(content) = fs::read_to_string(&res_path) {{
            let _ = fs::remove_file(&req_path);
            let _ = fs::remove_file(&res_path);
            return content;
        }}
    }}
    String::new()
}}

fn main() {{
{}
}}
"#, filtered_items.join("\n").replace("std::io::stdin()", "newt_io::stdin()").replace("io::stdin()", "newt_io::stdin()"), stmts.join("\n").replace("std::io::stdin()", "newt_io::stdin()").replace("io::stdin()", "newt_io::stdin()"));
        }
        
        if has_user_main {
            // User provided main and no loose statements. Use user's main.
            return items.join("\n").replace("std::io::stdin()", "newt_io::stdin()").replace("io::stdin()", "newt_io::stdin()");
        }
        
        // No statements and no user main. Generate empty main to ensure compilation.
        format!(r#"
#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]
{}

// Newt Input Support
use std::io::Read;
use std::io::Write;
use std::thread;
use std::time::Duration;
use std::fs;
use std::env;

mod newt_io {{
    use std::io;
    pub struct Stdin;
    pub fn stdin() -> Stdin {{ Stdin }}
    impl Stdin {{
        pub fn read_line(&self, buf: &mut String) -> io::Result<usize> {{
            let content = super::input();
            buf.push_str(&content);
            if !buf.ends_with('\n') {{
                buf.push('\n');
            }}
            Ok(content.len() + 1)
        }}
    }}
}}

fn input() -> String {{
    let temp_dir = env::temp_dir();
    let req_path = temp_dir.join("newt_web_input_req");
    let res_path = temp_dir.join("newt_web_input_res");

    if res_path.exists() {{
        let _ = fs::remove_file(&res_path);
    }}

    // Write request
    if let Ok(_) = fs::write(&req_path, "") {{
        // Wait for response
        while !res_path.exists() {{
            thread::sleep(Duration::from_millis(100));
        }}
        
        if let Ok(content) = fs::read_to_string(&res_path) {{
            let _ = fs::remove_file(&req_path);
            let _ = fs::remove_file(&res_path);
            return content;
        }}
    }}
    String::new()
}}

fn main() {{
}}
"#, items.join("\n").replace("std::io::stdin()", "newt_io::stdin()").replace("io::stdin()", "newt_io::stdin()"))
    }

    fn compile_and_run(&self, source: &str) -> Result<KernelResponse, String> {
        // Check for external dependencies
        let mut dependencies = std::collections::HashSet::new();
        let use_regex = regex::Regex::new(r"(?m)^\s*use\s+([a-zA-Z0-9_]+)").unwrap();
        let extern_regex = regex::Regex::new(r"(?m)^\s*extern\s+crate\s+([a-zA-Z0-9_]+)").unwrap();

        for cap in use_regex.captures_iter(source) {
            let dep = cap[1].to_string();
            if dep != "std" && dep != "core" && dep != "alloc" && dep != "crate" && dep != "super" && dep != "self" {
                dependencies.insert(dep);
            }
        }
        for cap in extern_regex.captures_iter(source) {
            let dep = cap[1].to_string();
            dependencies.insert(dep);
        }

        if dependencies.is_empty() {
            let temp_dir = std::env::temp_dir();
            let file_name = format!("newt_rust_{}.rs", uuid::Uuid::new_v4());
            let file_path = temp_dir.join(&file_name);
            let bin_path = temp_dir.join(format!("newt_rust_bin_{}", uuid::Uuid::new_v4()));

            std::fs::write(&file_path, source).map_err(|e| e.to_string())?;

            let compile_output = Command::new("rustc")
                .arg(&file_path)
                .arg("-o")
                .arg(&bin_path)
                .output()
                .map_err(|e| format!("Failed to run rustc: {}", e))?;

            if !compile_output.status.success() {
                let _ = std::fs::remove_file(&file_path);
                return Err(String::from_utf8_lossy(&compile_output.stderr).to_string());
            }

            let run_output = Command::new(&bin_path)
                .output()
                .map_err(|e| format!("Failed to run binary: {}", e))?;

            let _ = std::fs::remove_file(&file_path);
            let _ = std::fs::remove_file(&bin_path);

            Ok(KernelResponse {
                stdout: String::from_utf8_lossy(&run_output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&run_output.stderr).to_string(),
                status: run_output.status.code(),
                display_data: None,
            })
        } else {
            let temp_dir = std::env::temp_dir();
            let project_dir = temp_dir.join("newt_rust_project");
            
            if !project_dir.exists() {
                std::fs::create_dir_all(&project_dir).map_err(|e| e.to_string())?;
                Command::new("cargo")
                    .arg("init")
                    .arg("--bin")
                    .current_dir(&project_dir)
                    .output()
                    .map_err(|e| format!("Failed to init cargo project: {}", e))?;
            }

            let cargo_toml_path = project_dir.join("Cargo.toml");
            let cargo_toml_content = std::fs::read_to_string(&cargo_toml_path).unwrap_or_default();
            
            for dep in dependencies {
                if !cargo_toml_content.contains(&format!("{} =", dep)) && !cargo_toml_content.contains(&format!("\"{}\"", dep)) {
                     let _ = Command::new("cargo")
                        .arg("add")
                        .arg(&dep)
                        .current_dir(&project_dir)
                        .output();
                }
            }

            let main_rs_path = project_dir.join("src").join("main.rs");
            std::fs::write(&main_rs_path, source).map_err(|e| e.to_string())?;

            // Build first to separate build warnings from runtime output
            let build_output = Command::new("cargo")
                .arg("build")
                .arg("--quiet")
                .current_dir(&project_dir)
                .output()
                .map_err(|e| format!("Failed to build cargo project: {}", e))?;

            if !build_output.status.success() {
                return Err(String::from_utf8_lossy(&build_output.stderr).to_string());
            }

            // Run the binary directly to control working directory
            // Assuming default binary name from cargo init (directory name)
            let binary_name = "newt_rust_project";
            let binary_path = project_dir.join("target").join("debug").join(binary_name);
            
            // Use current working directory of the server process
            let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

            let run_output = Command::new(&binary_path)
                .current_dir(&current_dir)
                .output()
                .map_err(|e| format!("Failed to run binary: {}", e))?;

            Ok(KernelResponse {
                stdout: String::from_utf8_lossy(&run_output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&run_output.stderr).to_string(),
                status: run_output.status.code(),
                display_data: None,
            })
        }
    }
}

pub fn get_or_init_rust_kernel() -> Result<std::sync::MutexGuard<'static, Option<RustKernel>>, String> {
    let mut kernel_guard = RUST_KERNEL.lock().map_err(|_| "Failed to lock kernel mutex".to_string())?;
    
    if kernel_guard.is_none() {
        let kernel = RustKernel::new();
        *kernel_guard = Some(kernel);
    }
    
    Ok(kernel_guard)
}

pub struct CKernel;

impl Kernel for CKernel {
    fn execute(&mut self, code: String, _language: Option<String>, context: Option<Vec<String>>, _client_type: Option<String>) -> Result<KernelResponse, String> {
        let mut full_history = context.unwrap_or_default();
        full_history.push(code);
        
        let source = self.construct_source(&full_history);
        
        self.compile_and_run(&source)
    }
}

impl CKernel {
    pub fn new() -> Self {
        Self
    }

    pub fn clear(&mut self) {
        // Stateless
    }

    fn construct_source(&self, history: &[String]) -> String {
        let mut final_source = String::new();
        let re_main = regex::Regex::new(r"(?m)int\s+main\s*\(").unwrap();
        
        for (i, block) in history.iter().enumerate() {
            let is_last = i == history.len() - 1;
            
            if re_main.is_match(block) {
                if is_last {
                    final_source.push_str(block);
                } else {
                    let renamed = re_main.replace(block, format!("int main_ignored_{}(", i));
                    final_source.push_str(&renamed);
                }
            } else {
                if is_last {
                    final_source.push_str(&format!("\nint main() {{\n{}\nreturn 0;\n}}", block));
                } else {
                    final_source.push_str(block);
                }
            }
            final_source.push('\n');
        }
        
        if !final_source.contains("#include <stdio.h>") {
            final_source = format!("#include <stdio.h>\n{}", final_source);
        }
        
        final_source
    }

    fn compile_and_run(&self, source: &str) -> Result<KernelResponse, String> {
        let temp_dir = std::env::temp_dir();
        let file_name = format!("newt_c_{}.c", uuid::Uuid::new_v4());
        let file_path = temp_dir.join(&file_name);
        let bin_path = temp_dir.join(format!("newt_c_bin_{}", uuid::Uuid::new_v4()));

        std::fs::write(&file_path, source).map_err(|e| e.to_string())?;

        let compile_output = Command::new("gcc")
            .arg(&file_path)
            .arg("-o")
            .arg(&bin_path)
            .output()
            .map_err(|e| format!("Failed to run gcc: {}", e))?;

        if !compile_output.status.success() {
            let _ = std::fs::remove_file(&file_path);
            return Err(String::from_utf8_lossy(&compile_output.stderr).to_string());
        }

        let run_output = Command::new(&bin_path)
            .output()
            .map_err(|e| format!("Failed to run binary: {}", e))?;

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&bin_path);

        Ok(KernelResponse {
            stdout: String::from_utf8_lossy(&run_output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&run_output.stderr).to_string(),
            status: run_output.status.code(),
            display_data: None,
        })
    }
}

pub fn get_or_init_c_kernel() -> Result<std::sync::MutexGuard<'static, Option<CKernel>>, String> {
    let mut kernel_guard = C_KERNEL.lock().map_err(|_| "Failed to lock kernel mutex".to_string())?;
    
    if kernel_guard.is_none() {
        let kernel = CKernel::new();
        *kernel_guard = Some(kernel);
    }
    
    Ok(kernel_guard)
}

pub struct CppKernel;

impl Kernel for CppKernel {
    fn execute(&mut self, code: String, _language: Option<String>, context: Option<Vec<String>>, _client_type: Option<String>) -> Result<KernelResponse, String> {
        let mut full_history = context.unwrap_or_default();
        full_history.push(code);
        
        let source = self.construct_source(&full_history);
        
        self.compile_and_run(&source)
    }
}

impl CppKernel {
    pub fn new() -> Self {
        Self
    }

    pub fn clear(&mut self) {
        // Stateless
    }

    fn construct_source(&self, history: &[String]) -> String {
        let mut final_source = String::new();
        let re_main = regex::Regex::new(r"(?m)int\s+main\s*\(").unwrap();
        
        for (i, block) in history.iter().enumerate() {
            let is_last = i == history.len() - 1;
            
            if re_main.is_match(block) {
                if is_last {
                    final_source.push_str(block);
                } else {
                    let renamed = re_main.replace(block, format!("int main_ignored_{}(", i));
                    final_source.push_str(&renamed);
                }
            } else {
                if is_last {
                    final_source.push_str(&format!("\nint main() {{\n{}\nreturn 0;\n}}", block));
                } else {
                    final_source.push_str(block);
                }
            }
            final_source.push('\n');
        }
        
        if !final_source.contains("#include <iostream>") {
            final_source = format!("#include <iostream>\n{}", final_source);
        }

        // Inject Newt Input Support
        let newt_header = r#"
#include <iostream>
#include <fstream>
#include <string>
#include <thread>
#include <chrono>
#include <filesystem>
#include <streambuf>
#include <vector>
#include <cstring>

namespace newt {
    class NewtStreamBuf : public std::streambuf {
        char buffer[1024];
    public:
        NewtStreamBuf() {
            setg(buffer, buffer, buffer);
        }

    protected:
        int underflow() override {
            if (gptr() < egptr()) {
                return traits_type::to_int_type(*gptr());
            }

            auto temp_dir = std::filesystem::temp_directory_path();
            auto req_path = temp_dir / "newt_web_input_req";
            auto res_path = temp_dir / "newt_web_input_res";

            if (std::filesystem::exists(res_path)) {
                std::filesystem::remove(res_path);
            }

            // Flush cout before waiting for input so user sees prompt
            std::cout.flush();

            {
                std::ofstream req(req_path);
                req << ""; 
            }

            while (!std::filesystem::exists(res_path)) {
                std::this_thread::sleep_for(std::chrono::milliseconds(100));
            }

            std::string input_data;
            {
                std::ifstream res(res_path);
                std::getline(res, input_data);
                input_data += "\n";
            }

            if (std::filesystem::exists(req_path)) std::filesystem::remove(req_path);
            if (std::filesystem::exists(res_path)) std::filesystem::remove(res_path);

            size_t len = input_data.length();
            if (len > sizeof(buffer)) len = sizeof(buffer);
            std::memcpy(buffer, input_data.c_str(), len);
            
            setg(buffer, buffer, buffer + len);

            return traits_type::to_int_type(*gptr());
        }
    };

    static NewtStreamBuf newt_buf;
    struct NewtInitializer {
        NewtInitializer() {
            std::cin.rdbuf(&newt_buf);
        }
    };
    static NewtInitializer newt_init;
}
"#;
        final_source = format!("{}\n{}", newt_header, final_source);
        
        final_source
    }

    fn compile_and_run(&self, source: &str) -> Result<KernelResponse, String> {
        let temp_dir = std::env::temp_dir();
        let file_name = format!("newt_cpp_{}.cpp", uuid::Uuid::new_v4());
        let file_path = temp_dir.join(&file_name);
        let bin_path = temp_dir.join(format!("newt_cpp_bin_{}", uuid::Uuid::new_v4()));

        std::fs::write(&file_path, source).map_err(|e| e.to_string())?;

        let compile_output = Command::new("clang++")
            .arg(&file_path)
            .arg("-std=c++17")
            .arg("-I/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/c++/v1")
            .arg("-o")
            .arg(&bin_path)
            .output()
            .map_err(|e| format!("Failed to run clang++: {}", e))?;

        if !compile_output.status.success() {
            let _ = std::fs::remove_file(&file_path);
            return Err(String::from_utf8_lossy(&compile_output.stderr).to_string());
        }

        let run_output = Command::new(&bin_path)
            .output()
            .map_err(|e| format!("Failed to run binary: {}", e))?;

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(&bin_path);

        Ok(KernelResponse {
            stdout: String::from_utf8_lossy(&run_output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&run_output.stderr).to_string(),
            status: run_output.status.code(),
            display_data: None,
        })
    }
}

pub fn get_or_init_cpp_kernel() -> Result<std::sync::MutexGuard<'static, Option<CppKernel>>, String> {
    let mut kernel_guard = CPP_KERNEL.lock().map_err(|_| "Failed to lock kernel mutex".to_string())?;
    
    if kernel_guard.is_none() {
        let kernel = CppKernel::new();
        *kernel_guard = Some(kernel);
    }
    
    Ok(kernel_guard)
}

pub struct GoKernel;

impl Kernel for GoKernel {
    fn execute(&mut self, code: String, _language: Option<String>, context: Option<Vec<String>>, _client_type: Option<String>) -> Result<KernelResponse, String> {
        let mut full_history = context.unwrap_or_default();
        full_history.push(code);
        
        let source = self.construct_source(&full_history);
        
        self.compile_and_run(&source)
    }
}

impl GoKernel {
    pub fn new() -> Self {
        Self
    }

    pub fn clear(&mut self) {
        // Stateless
    }

    fn construct_source(&self, history: &[String]) -> String {
        let mut final_source = String::new();
        let re_main = regex::Regex::new(r"(?m)func\s+main\s*\(").unwrap();
        
        let mut imports = std::collections::HashSet::new();
        let mut top_level_decls = String::new();
        let mut main_body = String::new();
        
        let re_package = regex::Regex::new(r"(?m)^package\s+main\s*").unwrap();
        let re_import_single = regex::Regex::new(r#"(?m)^import\s+"([^"]+)""#).unwrap();
        let re_import_multi = regex::Regex::new(r"(?m)^import\s+\(([^)]+)\)").unwrap();
        
        for (i, block) in history.iter().enumerate() {
            let is_last = i == history.len() - 1;
            let mut processed_block = block.clone();
            
            // Remove package main
            processed_block = re_package.replace_all(&processed_block, "").to_string();
            
            // Extract imports
            for cap in re_import_single.captures_iter(&processed_block) {
                imports.insert(cap[1].to_string());
            }
            processed_block = re_import_single.replace_all(&processed_block, "").to_string();
            
            for cap in re_import_multi.captures_iter(&processed_block) {
                let import_block = &cap[1];
                for line in import_block.lines() {
                    let line = line.trim();
                    if line.starts_with('"') {
                        let import = line.trim_matches('"');
                        imports.insert(import.to_string());
                    }
                }
            }
            processed_block = re_import_multi.replace_all(&processed_block, "").to_string();
            
            // Heuristic: Check if block starts with top-level keywords
            let trimmed = processed_block.trim();
            
            // Split block into lines to handle mixed declarations and statements
            let lines: Vec<&str> = processed_block.lines().collect();
            let mut is_mixed = false;
            
            // If it starts with a declaration keyword but has multiple lines or semicolons, it might be mixed
            if trimmed.starts_with("type ") || trimmed.starts_with("func ") || trimmed.starts_with("const ") || trimmed.starts_with("var ") {
                // Check if it's a simple one-line declaration or a block
                // If it contains semicolons (not in quotes), it might be multiple statements
                // Simple check for semicolon
                if processed_block.contains(';') {
                    is_mixed = true;
                }
                // If multiple lines, check if subsequent lines are statements
                if lines.len() > 1 {
                    // This is hard to detect perfectly without a parser.
                    // But if we see "var x int\nx = 5", the second line is a statement.
                    // If we see "func main() {\n...}", it's a function.
                    if !trimmed.starts_with("func ") {
                        is_mixed = true;
                    }
                }
            }

            if is_mixed {
                // Process line by line / statement by statement
                // This is a simplified parser.
                // We split by newline and semicolon, but respect quotes/braces?
                // For now, let's just split by newline and semicolon if not in quotes.
                
                // Helper to split by char respecting quotes
                fn split_respecting_quotes(s: &str, delimiter: char) -> Vec<String> {
                    let mut result = Vec::new();
                    let mut current = String::new();
                    let mut in_quote = false;
                    let mut escape = false;
                    
                    for c in s.chars() {
                        if escape {
                            current.push(c);
                            escape = false;
                        } else if c == '\\' {
                            current.push(c);
                            escape = true;
                        } else if c == '"' {
                            current.push(c);
                            in_quote = !in_quote;
                        } else if c == delimiter && !in_quote {
                            result.push(current.trim().to_string());
                            current.clear();
                        } else {
                            current.push(c);
                        }
                    }
                    if !current.trim().is_empty() {
                        result.push(current.trim().to_string());
                    }
                    result
                }

                let stmts = split_respecting_quotes(&processed_block, ';');
                for stmt in stmts {
                    let stmt_lines = split_respecting_quotes(&stmt, '\n');
                    for line in stmt_lines {
                        let trimmed_line = line.trim();
                        if trimmed_line.is_empty() { continue; }
                        
                        if trimmed_line.starts_with("type ") || trimmed_line.starts_with("func ") || trimmed_line.starts_with("const ") || trimmed_line.starts_with("var ") {
                             if trimmed_line.starts_with("func main") {
                                 // Handle main specially as before
                                 if is_last {
                                     if let Some(start) = line.find('{') {
                                         if let Some(end) = line.rfind('}') {
                                             if start < end {
                                                 let body = &line[start+1..end];
                                                 main_body.push_str(body);
                                                 main_body.push('\n');
                                             }
                                         }
                                     }
                                 } else {
                                     let renamed = line.replace("func main", &format!("func main_ignored_{}", i));
                                     top_level_decls.push_str(&renamed);
                                     top_level_decls.push('\n');
                                 }
                             } else {
                                 top_level_decls.push_str(&line);
                                 top_level_decls.push('\n');
                             }
                        } else {
                            main_body.push_str(&line);
                            main_body.push('\n');
                        }
                    }
                }
            } else if trimmed.starts_with("type ") || trimmed.starts_with("func ") || trimmed.starts_with("const ") || trimmed.starts_with("var ") {
                if re_main.is_match(&processed_block) {
                    if is_last {
                        // assuming standard formatting "func main() {"
                        if let Some(start) = processed_block.find('{') {
                            if let Some(end) = processed_block.rfind('}') {
                                if start < end {
                                    let body = &processed_block[start+1..end];
                                    main_body.push_str(body);
                                    main_body.push('\n');
                                }
                            }
                        }
                    } else {
                        // If this block is NOT last, and has main, rename it.
                        let renamed = re_main.replace(&processed_block, format!("func main_ignored_{}(", i));
                        top_level_decls.push_str(&renamed);
                        top_level_decls.push('\n');
                    }
                } else {
                    // Regular declaration
                    top_level_decls.push_str(&processed_block);
                    top_level_decls.push('\n');
                }
            } else {
                // Statements. Append to main_body.
                main_body.push_str(&processed_block);
                main_body.push('\n');
            }
        }
        
        // Check if we need fmt
        if (main_body.contains("fmt.") || top_level_decls.contains("fmt.")) && !imports.contains("fmt") {
            imports.insert("fmt".to_string());
        }
        
        final_source.push_str("package main\n\n");
        
        if !imports.is_empty() {
            final_source.push_str("import (\n");
            for imp in imports {
                final_source.push_str(&format!("\t\"{}\"\n", imp));
            }
            // Add Newt imports
            final_source.push_str("\t\"os\"\n");
            final_source.push_str("\t\"path/filepath\"\n");
            final_source.push_str("\t\"time\"\n");
            final_source.push_str("\t\"io/ioutil\"\n");
            final_source.push_str(")\n");
        } else {
            final_source.push_str("import (\n");
            final_source.push_str("\t\"os\"\n");
            final_source.push_str("\t\"path/filepath\"\n");
            final_source.push_str("\t\"time\"\n");
            final_source.push_str("\t\"io/ioutil\"\n");
            final_source.push_str("\t\"fmt\"\n"); // Ensure fmt is available for our wrappers
            final_source.push_str(")\n");
        }

        // Perform replacements for Input Support
        let processed_decls = top_level_decls
            .replace("fmt.Scan(", "newt_scan(")
            .replace("fmt.Scanln(", "newt_scanln(")
            .replace("fmt.Scanf(", "newt_scanf(")
            .replace("bufio.NewReader(os.Stdin)", "bufio.NewReader(newtStdin)")
            .replace("bufio.NewScanner(os.Stdin)", "bufio.NewScanner(newtStdin)");

        final_source.push_str(&processed_decls);
        
        // Add Newt Input Helper
        final_source.push_str(r#"
func input() string {
    tempDir := os.TempDir()
    reqPath := filepath.Join(tempDir, "newt_web_input_req")
    resPath := filepath.Join(tempDir, "newt_web_input_res")

    if _, err := os.Stat(resPath); err == nil {
        os.Remove(resPath)
    }

    if err := ioutil.WriteFile(reqPath, []byte(""), 0644); err == nil {
        for {
            if _, err := os.Stat(resPath); err == nil {
                break
            }
            time.Sleep(100 * time.Millisecond)
        }
        
        if content, err := ioutil.ReadFile(resPath); err == nil {
            os.Remove(reqPath)
            os.Remove(resPath)
            return string(content)
        }
    }
    return ""
}

type NewtInputReader struct {
    buffer string
    pos    int
}

func (r *NewtInputReader) Read(p []byte) (n int, err error) {
    if r.pos >= len(r.buffer) {
        r.buffer = input() + "\n"
        r.pos = 0
    }
    n = copy(p, r.buffer[r.pos:])
    r.pos += n
    return n, nil
}

var newtStdin = &NewtInputReader{}

func newt_scan(a ...interface{}) (n int, err error) {
    return fmt.Fscan(newtStdin, a...)
}

func newt_scanln(a ...interface{}) (n int, err error) {
    return fmt.Fscanln(newtStdin, a...)
}

func newt_scanf(format string, a ...interface{}) (n int, err error) {
    return fmt.Fscanf(newtStdin, format, a...)
}
"#);
        
        let processed_body = main_body
            .replace("fmt.Scan(", "newt_scan(")
            .replace("fmt.Scanln(", "newt_scanln(")
            .replace("fmt.Scanf(", "newt_scanf(")
            .replace("bufio.NewReader(os.Stdin)", "bufio.NewReader(newtStdin)")
            .replace("bufio.NewScanner(os.Stdin)", "bufio.NewScanner(newtStdin)");

        final_source.push_str("\nfunc main() {\n");
        final_source.push_str(&processed_body);
        final_source.push_str("\n}\n");
        
        // println!("DEBUG GO SOURCE:\n{}", final_source);
        
        final_source
    }

    fn compile_and_run(&self, source: &str) -> Result<KernelResponse, String> {
        let temp_dir = std::env::temp_dir();
        let file_name = format!("newt_go_{}.go", uuid::Uuid::new_v4());
        let file_path = temp_dir.join(&file_name);
        
        std::fs::write(&file_path, source).map_err(|e| e.to_string())?;

        let run_output = Command::new("go")
            .arg("run")
            .arg(&file_path)
            .output()
            .map_err(|e| format!("Failed to run go: {}", e))?;

        let _ = std::fs::remove_file(&file_path);

        Ok(KernelResponse {
            stdout: String::from_utf8_lossy(&run_output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&run_output.stderr).to_string(),
            status: run_output.status.code(),
            display_data: None,
        })
    }
}

pub fn get_or_init_go_kernel() -> Result<std::sync::MutexGuard<'static, Option<GoKernel>>, String> {
    let mut kernel_guard = GO_KERNEL.lock().map_err(|_| "Failed to lock kernel mutex".to_string())?;
    
    if kernel_guard.is_none() {
        let kernel = GoKernel::new();
        *kernel_guard = Some(kernel);
    }
    
    Ok(kernel_guard)
}

