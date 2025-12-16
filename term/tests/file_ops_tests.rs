use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use newts::server::app;
use tower::ServiceExt; // for `oneshot`
use std::fs;
use directories::ProjectDirs;
use std::path::PathBuf;

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

#[tokio::test]
async fn test_delete_file() {
    let app = app();
    let filename = "test_delete_me_unique.newt";
    let mut path = get_app_dir();
    path.push(filename);

    // Create the file
    fs::write(&path, "test content").expect("Failed to create test file");
    assert!(path.exists());

    // Delete the file via API
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/files/delete")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({
                    "path": filename
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify file is gone
    assert!(!path.exists());
}
