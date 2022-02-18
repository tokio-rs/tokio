use tests_build::tokio;

// This should parse but fail since default values for const generic parameters
// are experimental.
//
// TODO(udoprog): might want to version constrain this check with `rustversion`
// since this won't always be experimental moving forward (!).
#[tokio::main]
async fn where_clause_const_generics<const T: usize = { 1 + 1 }>() {}

fn main() {}
