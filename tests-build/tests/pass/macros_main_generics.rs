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
async fn joint_bracket_in_generic<T: Iterator<Item = Option<fn() -> ()>>>(_: T) {}

struct GroupsInReturnPosition<const N: usize, const U: usize>;

// Tests both bracket groups `<{inner}>` and braces `{<inner>}` in the return
// position. The latter which should already be skipped over as part of the
// angle bracket processing.
#[tokio::main]
async fn groups_in_return_position() -> GroupsInReturnPosition<1, { 2 + 1 }> {
    todo!()
}

fn main() {}
