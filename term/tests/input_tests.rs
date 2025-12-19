use newts::server::kernel::{PythonKernel, NodeKernel, CppKernel, RustKernel, GoKernel, Kernel};
use std::thread;
use std::time::Duration;
use std::fs;
use std::env;

fn simulate_input(input_str: &str) {
    let temp_dir = env::temp_dir();
    let req_path = temp_dir.join("newt_web_input_req");
    let res_path = temp_dir.join("newt_web_input_res");

    // Wait for request
    let start = std::time::Instant::now();
    println!("Test: Waiting for request at {:?}", req_path);
    while !req_path.exists() {
        if start.elapsed().as_secs() > 30 {
            panic!("Timeout waiting for input request at {:?}", req_path);
        }
        thread::sleep(Duration::from_millis(100));
    }
    println!("Test: Found request");

    // Write response
    fs::write(&res_path, input_str).expect("Failed to write response");
    println!("Test: Wrote response to {:?}", res_path);
}

#[test]
fn test_python_input() {
    unsafe { std::env::set_var("NEWT_TEST_MODE", "1"); }
    let mut kernel = PythonKernel::new().expect("Failed to init kernel");
    
    let code = "x = input('Enter: '); print(f'Got: {x}')".to_string();
    
    let handle = thread::spawn(move || {
        kernel.execute(code, None, None, None).expect("Exec failed")
    });

    simulate_input("Hello Python");

    let resp = handle.join().expect("Thread panicked");
    assert_eq!(resp.status, Some(0));
    assert!(resp.stdout.contains("Got: Hello Python"));
}

#[test]
fn test_node_input() {
    let mut kernel = NodeKernel::new().expect("Failed to init kernel");
    
    let code = "const x = input('Enter: '); console.log(`Got: ${x}`);".to_string();
    
    let handle = thread::spawn(move || {
        kernel.execute(code, None, None, None).expect("Exec failed")
    });

    simulate_input("Hello Node");

    let resp = handle.join().expect("Thread panicked");
    assert_eq!(resp.status, Some(0));
    assert!(resp.stdout.contains("Got: Hello Node"));
}

#[test]
fn test_cpp_input() {
    let mut kernel = CppKernel::new();
    
    let code = "std::string x; std::cin >> x; std::cout << \"Got: \" << x;".to_string();
    
    let handle = thread::spawn(move || {
        kernel.execute(code, None, None, None).expect("Exec failed")
    });

    simulate_input("HelloCpp");

    let resp = handle.join().expect("Thread panicked");
    assert_eq!(resp.status, Some(0));
    assert!(resp.stdout.contains("Got: HelloCpp"));
}

#[test]
fn test_rust_input() {
    let mut kernel = RustKernel::new();
    
    // Use standard std::io::stdin
    let code = "use std::io; let mut x = String::new(); io::stdin().read_line(&mut x).unwrap(); println!(\"Got: {}\", x.trim());".to_string();
    
    let handle = thread::spawn(move || {
        kernel.execute(code, None, None, None).expect("Exec failed")
    });

    simulate_input("HelloRust");

    let resp = handle.join().expect("Thread panicked");
    assert_eq!(resp.status, Some(0));
    assert!(resp.stdout.contains("Got: HelloRust"));
}

#[test]
fn test_go_input() {
    let mut kernel = GoKernel::new();
    
    // Use standard fmt.Scan with short declaration to avoid top-level var issue
    let code = "x := \"\"; fmt.Scan(&x); fmt.Printf(\"Got: %s\", x)".to_string();
    
    let handle = thread::spawn(move || {
        kernel.execute(code, None, None, None).expect("Exec failed")
    });

    simulate_input("HelloGo");

    let resp = handle.join().expect("Thread panicked");
    assert_eq!(resp.status, Some(0));
    assert!(resp.stdout.contains("Got: HelloGo"));
}

