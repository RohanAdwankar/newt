use regex::Regex;

fn main() {
    let context = "fn main() { println!(\"Hello, world!\"); }";
    let code = "println!(\"hi\");";

    let re_context_main = Regex::new(r"(?m)fn\s+main").unwrap();
    
    if re_context_main.is_match(context) {
        println!("Context match!");
        let processed_context = re_context_main.replace_all(context, "fn main_ignored").to_string();
        println!("Processed context: {}", processed_context);
        
        let re_code_main = Regex::new(r"(?m)^\s*(?:pub\s+)?fn\s+main\s*\(").unwrap();
        let final_code = if re_code_main.is_match(code) {
            code.to_string()
        } else {
            format!("fn main() {{\n{}\n}}", code)
        };
        println!("Final code: {}", final_code);
        
        let source = format!("{}\n{}", processed_context, final_code);
        println!("Source:\n{}", source);
    } else {
        println!("No context match");
    }
}
