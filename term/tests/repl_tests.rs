use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use newts::server::app;
use newts::{CommandRequest, CommandResponse};
use newts::server::kernel::{RUST_KERNEL, C_KERNEL, CPP_KERNEL, GO_KERNEL};
use tower::ServiceExt; // for `oneshot`
use http_body_util::BodyExt; // for `collect`

fn clear_kernels() {
    if let Ok(mut guard) = RUST_KERNEL.lock() {
        if let Some(kernel) = guard.as_mut() {
            kernel.clear();
        }
    }
    if let Ok(mut guard) = C_KERNEL.lock() {
        if let Some(kernel) = guard.as_mut() {
            kernel.clear();
        }
    }
    if let Ok(mut guard) = CPP_KERNEL.lock() {
        if let Some(kernel) = guard.as_mut() {
            kernel.clear();
        }
    }
    if let Ok(mut guard) = GO_KERNEL.lock() {
        if let Some(kernel) = guard.as_mut() {
            kernel.clear();
        }
    }
}

#[tokio::test]
async fn test_echo_command() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "echo hello".to_string(),
                    language: None,
                    context: None,
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(resp.stdout.trim(), "hello");
}

#[tokio::test]
async fn test_cargo_version() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "cargo --version".to_string(),
                    language: None,
                    context: None,
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    
    assert!(resp.stdout.contains("cargo"));
}

#[tokio::test]
async fn test_state_persistence_fail() {
    // This test demonstrates that state is NOT persisted between commands
    // which is a limitation of the current implementation (no REPL session).
    let app = app();

    // 1. Change directory
    let _ = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "cd /".to_string(),
                    language: None,
                    context: None,
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // 2. Check directory
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "pwd".to_string(),
                    language: None,
                    context: None,
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    
    // It should NOT be "/" because the previous `cd` happened in a separate process
    assert_ne!(resp.stdout.trim(), "/");
}

#[tokio::test]
async fn test_quoted_arguments_fail() {
    // This test demonstrates that quoted arguments are NOT handled correctly
    // by the naive split_whitespace() implementation.
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "echo \"hello world\"".to_string(),
                    language: None,
                    context: None,
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    
    // Naive split will result in: echo, "hello, world"
    // Output will likely include the quotes: "hello world"
    // A proper shell would output: hello world
    assert_eq!(resp.stdout.trim(), "\"hello world\"");
}

#[tokio::test]
async fn test_rust_snippet_execution() {
    clear_kernels();
    let app = app();

    let code = r#"
        const foo: &str = "bar";
        println!("{}", foo);
    "#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: code.to_string(),
                    language: Some("rust".to_string()),
                    context: None,
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(resp.stdout.trim(), "bar");
    assert!(resp.stderr.is_empty());
}

#[tokio::test]
async fn test_python_statefulness() {
    let app = app();

    // Step 1: Set variable
    let response1 = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "x = 42".to_string(),
                    language: Some("python".to_string()),
                    context: None,
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response1.status(), StatusCode::OK);

    // Step 2: Read variable
    let response2 = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "print(x)".to_string(),
                    language: Some("python".to_string()),
                    context: None,
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::OK);

    let body = response2.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(resp.stdout.trim(), "42");
}

#[tokio::test]
async fn test_rust_statefulness() {
    clear_kernels();
    let app = app();

    // Cell 1: let x = 100;
    // Cell 2: println!("{}", x);
    // Context: let x = 100;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "println!(\"{}\", x);".to_string(),
                    language: Some("rust".to_string()),
                    context: Some(vec!["let x = 100;".to_string()]),
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(resp.stdout.trim(), "100");
}

#[tokio::test]
async fn test_c_statefulness() {
    clear_kernels();
    let app = app();

    // Cell 1: int x = 55; (Global)
    // Cell 2: int main() { printf("%d", x); return 0; }
    // Context: int x = 55;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "int main() { printf(\"%d\", x); return 0; }".to_string(),
                    language: Some("c".to_string()),
                    context: Some(vec!["#include <stdio.h>\nint x = 55;".to_string()]),
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(resp.stdout.trim(), "55");
}

#[tokio::test]
async fn test_rust_mixed_context() {
    clear_kernels();
    let app = app();

    // Scenario 1: Context has main, Code is snippet
    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "println!(\"hi\");".to_string(),
                    language: Some("rust".to_string()),
                    context: Some(vec!["fn main() { println!(\"Hello, world!\"); }".to_string()]),
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.stdout.trim(), "hi");

    // Scenario 2: Context is snippet, Code is snippet
    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "println!(\"curr\");".to_string(),
                    language: Some("rust".to_string()),
                    context: Some(vec!["println!(\"prev\");".to_string()]),
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    // Output should contain both because they are combined in one main
    assert!(resp.stdout.contains("prev"));
    assert!(resp.stdout.contains("curr"));
}

#[tokio::test]
async fn test_rust_comment_main() {
    clear_kernels();
    let app = app();

    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "println!(\"hi\");".to_string(),
                    language: Some("rust".to_string()),
                    context: Some(vec!["// fn main".to_string()]),
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.stdout.trim(), "hi");
}

#[tokio::test]
async fn test_python_matplotlib_plot() {
    let app = app();

    let code = r#"
import matplotlib.pyplot as plt
plt.plot([1, 2, 3], [1, 2, 3])
plt.show()
"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: code.to_string(),
                    language: Some("python".to_string()),
                    context: None,
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    
    if resp.status != Some(0) {
        println!("Stderr: {}", resp.stderr);
        println!("Stdout: {}", resp.stdout);
    }
    assert_eq!(resp.status, Some(0));
    assert!(resp.display_data.is_some());
    let display_data = resp.display_data.unwrap();
    assert!(!display_data.is_empty());
    
    // Check for image data (Base64)
    let has_image = display_data.iter().any(|d| {
        d.data.iter().any(|(k, v)| {
            if k.starts_with("image/") {
                // In persistent kernel, we return Base64 string
                if let Some(b64) = v.as_str() {
                    // Just check it's a reasonably long string
                    b64.len() > 100
                } else {
                    false
                }
            } else {
                false
            }
        })
    });
    assert!(has_image, "Should have image output (Base64)");
}

#[tokio::test]
async fn test_python_display_json() {
    let app = app();

    let code = r#"
class MyJSON:
    def _repr_json_(self):
        return {"foo": "bar"}

display(MyJSON())
"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: code.to_string(),
                    language: Some("python".to_string()),
                    context: None,
                    client_type: None,
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(resp.status, Some(0));
    assert!(resp.display_data.is_some());
    let display_data = resp.display_data.unwrap();
    
    // Check for json data
    let has_json = display_data.iter().any(|d| {
        d.data.contains_key("application/json")
    });
    assert!(has_json, "Should have json output");
}

