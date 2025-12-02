use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use core_api::{app, CommandRequest, CommandResponse};
use tower::ServiceExt; // for `oneshot`
use http_body_util::BodyExt; // for `collect`

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

    // Simulate running two cells.
    // Cell 1: x = 42
    // Cell 2: print(x)
    // We pass "x = 42" as context to Cell 2.

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&CommandRequest {
                    command: "print(x)".to_string(),
                    language: Some("python".to_string()),
                    context: Some("x = 42".to_string()),
                }).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CommandResponse = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(resp.stdout.trim(), "42");
}

#[tokio::test]
async fn test_rust_statefulness() {
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
                    context: Some("let x = 100;".to_string()),
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
                    context: Some("#include <stdio.h>\nint x = 55;".to_string()),
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
