use newts::server::kernel::{PythonKernel, NodeKernel, CppKernel, Kernel};
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
    while !req_path.exists() {
        if start.elapsed().as_secs() > 30 {
            panic!("Timeout waiting for input request");
        }
        thread::sleep(Duration::from_millis(100));
    }

    // Write response
    fs::write(&res_path, input_str).expect("Failed to write response");
}

#[test]
fn test_python_input() {
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
