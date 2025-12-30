use crate::{Cell, CellType};

pub fn parse_markdown(content: &str) -> Vec<Cell> {
    let mut cells = Vec::new();
    let mut current_text = String::new();
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("```") {
            // Flush current text as Markdown cell
            if !current_text.trim().is_empty() {
                 let id = uuid::Uuid::new_v4().to_string();
                 // Remove leading newline if present
                 let content = if current_text.starts_with('\n') {
                     &current_text[1..]
                 } else {
                     &current_text
                 };
                 cells.push(Cell {
                     id,
                     content: content.trim_end().to_string(),
                     output: String::new(),
                     cell_type: CellType::Markdown,
                     polling_interval: None,
                     last_run: None,
                 });
                 current_text.clear();
            }

            // Parse Code Block
            let lang_tag = line.trim_start().trim_start_matches("```").trim();

            let cell_type = match lang_tag {
                "rust" => CellType::Rust,
                "python" | "py" => CellType::Python,
                "javascript" | "js" => CellType::JavaScript,
                "typescript" | "ts" => CellType::TypeScript,
                "c" => CellType::C,
                "cpp" | "c++" => CellType::Cpp,
                "go" => CellType::Go,
                "bash" | "sh" | "shell" => CellType::Shell,
                _ => CellType::Shell, //if codeblock with no specific tag then it's shell
            };

            let mut code_content = String::new();
            while let Some(code_line) = lines.peek() {
                if code_line.trim_start().starts_with("```") {
                    lines.next(); // Consume closing fence
                    break;
                }
                code_content.push_str(lines.next().unwrap());
                code_content.push('\n');
            }
            
            // Parse Output
            let mut output_content = String::new();
            while let Some(out_line) = lines.peek() {
                if out_line.starts_with(">> ") {
                    output_content.push_str(&out_line[3..]);
                    output_content.push('\n');
                    lines.next();
                } else {
                    break;
                }
            }

            let id = uuid::Uuid::new_v4().to_string();
            
            if cell_type == CellType::Markdown {
                cells.push(Cell {
                    id,
                    content: code_content.trim_end().to_string(),
                    output: String::new(),
                    cell_type,
                    polling_interval: None,
                    last_run: None,
                });
            } else {
                cells.push(Cell {
                    id,
                    content: code_content.trim_end().to_string(),
                    output: output_content.trim_end().to_string(),
                    cell_type,
                    polling_interval: None,
                    last_run: None,
                });
            }

        } else {
            current_text.push_str(line);
            current_text.push('\n');
        }
    }
    
    // Flush remaining text
    if !current_text.trim().is_empty() {
         let id = uuid::Uuid::new_v4().to_string();
         // Remove leading newline if present
         let content = if current_text.starts_with('\n') {
             &current_text[1..]
         } else {
             &current_text
         };
         cells.push(Cell {
             id,
             content: content.trim_end().to_string(),
             output: String::new(),
             cell_type: CellType::Markdown,
             polling_interval: None,
             last_run: None,
         });
    }

    // Ensure at least one cell
    if cells.is_empty() {
        cells.push(Cell {
            id: uuid::Uuid::new_v4().to_string(),
            content: String::new(),
            output: String::new(),
            cell_type: CellType::Shell,
            polling_interval: None,
            last_run: None,
        });
    }

    cells
}

pub fn to_markdown(cells: &[Cell]) -> String {
    let mut output = String::new();
    
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }

        if cell.cell_type == CellType::Markdown {
            output.push_str(&cell.content);
            output.push('\n');
        } else {
            let lang = match cell.cell_type {
                CellType::Rust => "rust",
                CellType::Python => "python",
                CellType::JavaScript => "javascript",
                CellType::TypeScript => "typescript",
                CellType::C => "c",
                CellType::Cpp => "cpp",
                CellType::Go => "go",
                CellType::Shell => "bash",
                CellType::Markdown => "markdown", 
            };
            
            output.push_str(&format!("```{}\n", lang));
            output.push_str(&cell.content);
            if !cell.content.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("```\n");
            
            if !cell.output.is_empty() {
                for line in cell.output.lines() {
                    output.push_str(&format!(">> {}\n", line));
                }
            }
        }
    }
    
    output
}
