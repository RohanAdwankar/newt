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
    pub fn new() -> Result<Self, String> {
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
    if _newt_client_type == "web":
        # Web mode: File-based coordination
        import time
        import os
        
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

    # Create a temporary file to store the input
    with tempfile.NamedTemporaryFile(mode='w+', delete=False) as tf:
        tf_path = tf.name

    # Create a temporary script to run in the external terminal
    # This script prompts the user and writes the result to the temp file
    input_script = f'''
import os
try:
    val = input("{{prompt}}")
    with open("{{tf_path}}", "w") as f:
        f.write(val)
except Exception as e:
    pass
'''
    
    with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as script_tf:
        script_tf.write(input_script)
        script_path = script_tf.name

    # Launch external terminal
    # Using osascript for macOS
    cmd = f'tell application "Terminal" to do script "python3 {{script_path}}; exit"'
    subprocess.run(['osascript', '-e', cmd], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    # Wait for the file to be written (polling)
    import time
    start_time = time.time()
    while time.time() - start_time < 300: # 5 minute timeout
        try:
            if os.path.getsize(tf_path) > 0:
                with open(tf_path, 'r') as f:
                    result = f.read()
                os.remove(tf_path)
                os.remove(script_path)
                return result
        except:
            pass
        time.sleep(0.1)
    
    return ""

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
            
            # We need to inject display into the globals or builtins for this run
            # But builtins.display is global. 
            builtins.display = _display
            builtins.input = _newt_input

            success = True
            with contextlib.redirect_stdout(stdout_capture), contextlib.redirect_stderr(stderr_capture):
                try:
                    # Execute code in the persistent globals dictionary
                    exec(code, _newt_globals)
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
        // We use "uv run" as in the original code
        let mut cmd = Command::new("uv");
        cmd.arg("run");
        cmd.arg("--with");
        cmd.arg("matplotlib");
        cmd.arg("python");
        cmd.arg(&script_path);

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit()); // Let stderr go to console for debugging the kernel itself

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

pub fn get_or_init_python_kernel() -> Result<std::sync::MutexGuard<'static, Option<PythonKernel>>, String> {
    let mut kernel_guard = PYTHON_KERNEL.lock().map_err(|_| "Failed to lock kernel mutex".to_string())?;
    
    if kernel_guard.is_none() {
        let kernel = PythonKernel::new()?;
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

const context = vm.createContext({
  console: customConsole,
  require: require,
  process: process,
  setTimeout: setTimeout,
  setInterval: setInterval,
  clearTimeout: clearTimeout,
  clearInterval: clearInterval
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
        
        for block in history {
            let trimmed = block.trim();
            if trimmed.starts_with("fn ") || trimmed.starts_with("struct ") || trimmed.starts_with("enum ") || trimmed.starts_with("impl ") || trimmed.starts_with("use ") || trimmed.starts_with("mod ") || trimmed.starts_with("type ") {
                if trimmed.starts_with("fn main") {
                    has_user_main = true;
                }
                items.push(block.as_str());
            } else {
                stmts.push(block.as_str());
            }
        }
        
        if !stmts.is_empty() {
            // If we have statements, we must wrap them in a main function.
            // To avoid conflict, we filter out any user-provided main.
            let filtered_items: Vec<&str> = items.into_iter()
                .filter(|i| !i.trim().starts_with("fn main"))
                .collect();
                
            return format!(r#"
{}

fn main() {{
{}
}}
"#, filtered_items.join("\n"), stmts.join("\n"));
        }
        
        if has_user_main {
            // User provided main and no loose statements. Use user's main.
            return items.join("\n");
        }
        
        // No statements and no user main. Generate empty main to ensure compilation.
        format!(r#"
{}

fn main() {{
}}
"#, items.join("\n"))
    }

    fn compile_and_run(&self, source: &str) -> Result<KernelResponse, String> {
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
            if trimmed.starts_with("type ") || trimmed.starts_with("func ") || trimmed.starts_with("const ") || trimmed.starts_with("var ") {
                // It's a declaration.
                // But wait, if it is "func main", we handle it differently.
                if re_main.is_match(&processed_block) {
                    if is_last {
                        // It's the main function for this run.
                        // We need to extract the body of main and put it in our main_body?
                        // Or just append it to top_level_decls?
                        // If we have multiple mains, we can't have them all at top level.
                        // We are building a SINGLE main.
                        // So if user provides main, we should take its body and put it in our main.
                        // Or we rename it?
                        // If we rename it, it's just another function.
                        // But we want its side effects (printing etc) to happen.
                        // So we should call it?
                        
                        // Let's stick to: rename old mains to main_ignored_i and call them?
                        // No, we don't want to re-run old mains.
                        // We only want to run the LAST main if provided.
                        // If previous blocks had main, we rename them so they don't conflict, but we don't call them.
                        
                        // If this block is NOT last, and has main, rename it.
                        let renamed = re_main.replace(&processed_block, format!("func main_ignored_{}(", i));
                        top_level_decls.push_str(&renamed);
                        top_level_decls.push('\n');
                    } else {
                        // It is last. It has main.
                        // We can just append it to top_level_decls.
                        // But we also have main_body from other blocks (if any).
                        // This is conflicting.
                        // If user provides main, they probably want to control execution.
                        // But if previous blocks were statements (wrapped in main), we want them to have run?
                        // No, we are re-running everything.
                        // So we want previous statements to run again?
                        // Yes, persistence means state is preserved.
                        // In "re-run history" model, we re-execute everything to rebuild state.
                        // So we MUST run previous statements.
                        
                        // So we need to merge all "main bodies".
                        // If a block was "func main() { ... }", we extract "..." and append to main_body.
                        // If a block was statements, we append to main_body.
                        
                        // Extract body of main
                        // This is hard with regex.
                        // Let's assume standard formatting "func main() {"
                        if let Some(start) = processed_block.find('{') {
                            if let Some(end) = processed_block.rfind('}') {
                                if start < end {
                                    let body = &processed_block[start+1..end];
                                    main_body.push_str(body);
                                    main_body.push('\n');
                                }
                            }
                        }
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
            final_source.push_str(")\n");
        }

        final_source.push_str(&top_level_decls);
        
        final_source.push_str("\nfunc main() {\n");
        final_source.push_str(&main_body);
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

