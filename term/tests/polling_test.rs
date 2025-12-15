use newts::{App, CellType, Cell};
use std::thread;
use std::time::Duration;
use std::fs;
use std::process::Command;

#[test]
fn test_polling_logic() {
    let mut app = App::new(None);
    let test_file = "polling_test.txt";
    let _ = fs::remove_file(test_file);

    // Cell 1: Create file
    app.cells.push(Cell {
        id: "1".to_string(),
        content: format!("echo 'start' > {}", test_file),
        output: "".to_string(),
        cell_type: CellType::Shell,
        polling_interval: None,
        last_run: None,
    });

    // Run cell 1 manually
    let _ = Command::new("sh").arg("-c").arg(&app.cells[0].content).output();

    // Cell 2: Append line
    app.cells.push(Cell {
        id: "2".to_string(),
        content: format!("echo 'line' >> {}", test_file),
        output: "".to_string(),
        cell_type: CellType::Shell,
        polling_interval: Some(1), // 1 second
        last_run: None,
    });

    // Simulate loop for 5 seconds
    // We sleep 1.1s to ensure we pass the 1s threshold
    for _ in 0..5 {
        let indices = app.check_polling();
        for i in indices {
            // Execute command manually since we don't have the server running in test
            let cell = &app.cells[i];
            let _ = Command::new("sh").arg("-c").arg(&cell.content).output();
        }
        thread::sleep(Duration::from_millis(1100));
    }

    // Verify content
    let content = fs::read_to_string(test_file).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    // 1 start line + 5 appended lines = 6 lines
    assert!(lines.len() >= 5, "Expected at least 5 lines, got {}", lines.len());
    
    let _ = fs::remove_file(test_file);
}
