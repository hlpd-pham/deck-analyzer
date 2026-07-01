use std::{
    fs::File,
    io::{BufRead, BufReader},
};
pub mod types;

pub fn read_json_file(path: &str) {
    let file = File::open(path).expect(&format!("cannot read_json_file: {}", path));
    let reader = BufReader::new(file);
    let limit = 5;
    let mut line_index = 0;

    for line_result in reader.lines() {
        if line_index == limit {
            break;
        }
        line_index += 1;
        match line_result {
            Ok(line) => {
                println!("{}", line)
            }
            Err(e) => {
                println!("Encounter error: {}", e);
            }
        }
    }
}
