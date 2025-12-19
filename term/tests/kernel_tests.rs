use newts::server::kernel::{PythonKernel, NodeKernel, RustKernel, CKernel, CppKernel, GoKernel, Kernel};

#[test]
fn test_python_kernel_persistence() {
    let mut kernel = PythonKernel::new().expect("Failed to init kernel");

    // 1. Set a variable
    let resp1 = kernel.execute("x = 42".to_string(), None, None, None).expect("Exec failed");
    assert_eq!(resp1.status, Some(0));

    // 2. Read it back
    let resp2 = kernel.execute("print(x)".to_string(), None, None, None).expect("Exec failed");
    assert_eq!(resp2.status, Some(0));
    assert_eq!(resp2.stdout.trim(), "42");
}

#[test]
fn test_node_kernel_persistence() {
    let mut kernel = NodeKernel::new().expect("Failed to init kernel");
    
    // 1. Set variable
    let resp1 = kernel.execute("var x = 42;".to_string(), None, None, None).expect("Exec failed");
    assert_eq!(resp1.status, Some(0));
    
    // 2. Read variable
    let resp2 = kernel.execute("console.log(x);".to_string(), None, None, None).expect("Exec failed");
    assert_eq!(resp2.status, Some(0));
    assert_eq!(resp2.stdout.trim(), "42");
}

#[test]
fn test_rust_kernel_persistence() {
    let mut kernel = RustKernel::new();
    
    // 1. Define struct
    let code1 = "struct Foo { x: i32 }".to_string();
    let resp1 = kernel.execute(code1.clone(), None, None, None).expect("Exec failed");
    assert_eq!(resp1.status, Some(0));
    
    // 2. Use struct
    let code2 = "let f = Foo { x: 42 }; println!(\"{}\", f.x);".to_string();
    let resp2 = kernel.execute(code2, None, Some(vec![code1]), None).expect("Exec failed");
    assert_eq!(resp2.status, Some(0));
    assert_eq!(resp2.stdout.trim(), "42");
}

#[test]
fn test_python_kernel_isolation() {
    let mut kernel = PythonKernel::new().expect("Failed to init kernel");

    // 1. Set variable
    let resp1 = kernel.execute("x = 100".to_string(), None, None, None).expect("Exec failed");
    assert_eq!(resp1.status, Some(0));

    // 2. Restart kernel (simulate isolation by creating new one)
    let mut kernel2 = PythonKernel::new().expect("Failed to init kernel");
    
    // 3. Check variable (should fail)
    let resp2 = kernel2.execute("print(x)".to_string(), None, None, None).expect("Exec failed");
    assert_ne!(resp2.status, Some(0));
}

#[test]
fn test_c_kernel_persistence() {
    let mut kernel = CKernel::new();
    
    // 1. Define global variable
    let code1 = "int x = 42;".to_string();
    let resp1 = kernel.execute(code1.clone(), None, None, None).expect("Exec failed");
    assert_eq!(resp1.status, Some(0));
    
    // 2. Use variable
    let code2 = "printf(\"%d\", x);".to_string();
    let resp2 = kernel.execute(code2, None, Some(vec![code1]), None).expect("Exec failed");
    assert_eq!(resp2.status, Some(0));
    assert_eq!(resp2.stdout.trim(), "42");
}

#[test]
fn test_cpp_kernel_persistence() {
    let mut kernel = CppKernel::new();
    
    // 1. Define class
    let code1 = "class Foo { public: int x; };".to_string();
    let resp1 = kernel.execute(code1.clone(), None, None, None).expect("Exec failed");
    if resp1.status != Some(0) {
        println!("Cpp Exec 1 failed: {}", resp1.stderr);
    }
    assert_eq!(resp1.status, Some(0));
    
    // 2. Use class
    let code2 = "Foo f; f.x = 42; std::cout << f.x;".to_string();
    let resp2 = kernel.execute(code2, None, Some(vec![code1]), None).expect("Exec failed");
    if resp2.status != Some(0) {
        println!("Cpp Exec 2 failed: {}", resp2.stderr);
    }
    assert_eq!(resp2.status, Some(0));
    assert_eq!(resp2.stdout.trim(), "42");
}

#[test]
fn test_go_kernel_persistence() {
    let mut kernel = GoKernel::new();
    
    // 1. Define struct
    let code1 = "type Foo struct { X int }".to_string();
    let resp1 = kernel.execute(code1.clone(), None, None, None).expect("Exec failed");
    if resp1.status != Some(0) {
        println!("Go Exec 1 failed: {}", resp1.stderr);
    }
    assert_eq!(resp1.status, Some(0));
    
    // 2. Use struct
    let code2 = "f := Foo{X: 42}; fmt.Println(f.X)".to_string();
    let resp2 = kernel.execute(code2, None, Some(vec![code1]), None).expect("Exec failed");
    if resp2.status != Some(0) {
        println!("Go Exec 2 failed: {}", resp2.stderr);
    }
    assert_eq!(resp2.status, Some(0));
    assert_eq!(resp2.stdout.trim(), "42");
}

#[test]
fn test_typescript_kernel_persistence() {
    let mut kernel = NodeKernel::new().expect("Failed to init kernel");
    
    // 1. Set variable with type annotation
    let code1 = "let x: number = 42;".to_string();
    let resp1 = kernel.execute(code1, Some("typescript".to_string()), None, None).expect("Exec failed");
    
    // If typescript is missing, it returns error.
    if resp1.status != Some(0) {
        println!("TS Exec 1 failed (likely missing typescript): {}", resp1.stderr);
        // We can't assert success if env is missing deps, but we can assert it handled it.
        return;
    }
    assert_eq!(resp1.status, Some(0));
    
    // 2. Read variable
    let code2 = "console.log(x);".to_string();
    let resp2 = kernel.execute(code2, Some("typescript".to_string()), None, None).expect("Exec failed");
    assert_eq!(resp2.status, Some(0));
    assert_eq!(resp2.stdout.trim(), "42");
}

#[test]
fn test_python_performance_persistence() {
    let mut kernel = PythonKernel::new().expect("Failed to init kernel");

    // 1. Real Work: Create a large list (5 million items)
    // This forces actual CPU usage and Memory allocation (~40MB+ in Python)
    let start = std::time::Instant::now();
    let code1 = "x = [i for i in range(5000000)]".to_string();
    let resp1 = kernel.execute(code1, None, None, None).expect("Exec failed");
    let duration1 = start.elapsed();
    
    assert_eq!(resp1.status, Some(0));
    println!("Python Cell 1 (Allocation) duration: {:?}", duration1);

    // 2. Access variable (should be instant)
    // If we were re-running history, this would take as long as Cell 1.
    let start2 = std::time::Instant::now();
    let code2 = "print(len(x))".to_string();
    let resp2 = kernel.execute(code2, None, None, None).expect("Exec failed");
    let duration2 = start2.elapsed();

    assert_eq!(resp2.status, Some(0));
    assert_eq!(resp2.stdout.trim(), "5000000");
    
    println!("Python Cell 2 (Access) duration: {:?}", duration2);
    
    // Access should be instant (< 100ms), whereas allocation takes time.
    assert!(duration2.as_millis() < 100, "Accessing existing memory should be instant");
}

#[test]
fn test_node_performance_persistence() {
    let mut kernel = NodeKernel::new().expect("Failed to init kernel");

    // 1. Real Work: Create large array (5 million items)
    let code1 = r#"
    var x = new Array(5000000).fill(0).map((_, i) => i);
    "#.to_string();
    
    let start = std::time::Instant::now();
    let resp1 = kernel.execute(code1, None, None, None).expect("Exec failed");
    let duration1 = start.elapsed();
    
    assert_eq!(resp1.status, Some(0));
    println!("Node Cell 1 (Allocation) duration: {:?}", duration1);

    // 2. Access variable
    let start2 = std::time::Instant::now();
    let code2 = "console.log(x.length);".to_string();
    let resp2 = kernel.execute(code2, None, None, None).expect("Exec failed");
    let duration2 = start2.elapsed();

    assert_eq!(resp2.status, Some(0));
    assert_eq!(resp2.stdout.trim(), "5000000");
    
    println!("Node Cell 2 (Access) duration: {:?}", duration2);
    assert!(duration2.as_millis() < 100, "Accessing existing memory should be instant");
}

#[test]
fn test_python_input_override() {
    let mut kernel = PythonKernel::new().expect("Failed to init kernel");
    
    // Check if input is overridden
    let code = "import builtins; print(builtins.input.__name__)".to_string();
    let resp = kernel.execute(code, None, None, None).expect("Exec failed");
    
    assert_eq!(resp.status, Some(0));
    assert_eq!(resp.stdout.trim(), "_newt_input");
}
