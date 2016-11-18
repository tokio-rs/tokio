// A cat-like utility that can be used as a subprocess to test I/O
// stream communication.
use std::io;
use std::io::Write;

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        stdin.read_line(&mut line).unwrap();
        if line.len() == 0 {
            break;
        }
        stdout.write(line.as_bytes()).unwrap();
    }
}
