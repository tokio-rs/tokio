use tests_build::tokio;

// This should parse but fail since default values for const generic parameters
// are experimental.
#[tokio::main]
async fn where_clause_const_generics<const T: usize = { 1 + 1 }>() {}

fn main() {}
