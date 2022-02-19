use tests_build::tokio;

use std::fmt::Debug;

#[tokio::main]
async fn single_parameter<T>() {}

// This should parse since we treat angle brackets as groups during parsing and
// simply skip over them.
#[tokio::main]
async fn where_clause<T>()
where
    T: Iterator,
    <T as Iterator>::Item: Debug,
{
}

#[tokio::main]
async fn join_bracket_in_return() -> Option<fn() -> ()> {
    todo!()
}

#[tokio::main]
async fn joint_bracket_in_generic<T: Iterator<Item = Option<fn() -> ()>>>(_: T) {
}

fn main() {}
