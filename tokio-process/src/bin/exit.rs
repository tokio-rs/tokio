#[allow(dead_code)]

fn main() {
    std::process::exit(std::env::args().nth(1).unwrap().parse().unwrap());
}
