//! A cat-like utility that can be used as a subprocess to test I/O
//! stream communication.

use std::io;
use std::io::Write;

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        stdin.read_line(&mut line).unwrap();
        if line.is_empty() {
            break;
        }
        stdout.write_all(line.as_bytes()).unwrap();
    }
    stdout.flush().unwrap();
}
