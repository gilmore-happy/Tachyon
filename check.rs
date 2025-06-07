use std::fs::File;
use std::io::{BufRead, BufReader};

fn main() {
    let file = File::open("/home/galt/Solana Trading Bot/src/main.rs").unwrap();
    let reader = BufReader::new(file);
    let mut count = 0;
    
    for (line_num, line) in reader.lines().enumerate() {
        let line = line.unwrap();
        if line.contains("JoinSet<()>") {
            count += 1;
            println!("Line {}: {}", line_num + 1, line);
        }
    }
    
    println!("Found {} occurrences of JoinSet declaration", count);
}