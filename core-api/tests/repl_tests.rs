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
