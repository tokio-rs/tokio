use std::future;

use tests_build::tokio;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // These names deliberately overlap with identifiers used by the macro's
    // expansion. They must continue to refer to the caller's bindings.
    let start = 1usize;
    let futures = 2usize;
    let disabled = 4usize;
    let mask = 8usize;
    let output = 16usize;
    let poll_fn = 32usize;
    let branch = 64usize;
    let cx = 128usize;

    let hygienic = tokio::select! {
        #[cfg(all())]
        value = future::ready(start + futures + disabled + mask + output + poll_fn + branch + cx) => value,
    };
    assert_eq!(hygienic, 255);

    let expected = if cfg!(feature = "full") { 1 } else { 2 };
    let selected = tokio::select! {
        #[cfg(feature = "full")]
        value = future::ready(1usize) => value,
        #[cfg(not(feature = "full"))]
        value = future::ready(2usize) => value,
    };
    assert_eq!(selected, expected);

    let unresolved = tokio::select! {
        #[cfg(any())]
        missing::Pattern(binding) = missing::future::<missing::Type>(missing_variable),
            if missing::precondition() => missing::handler(binding),
        value = future::ready(3usize) => value,
    };
    assert_eq!(unresolved, 3);

    let nested = tokio::select! {
        #[cfg(all())]
        outer = future::ready(4usize) => tokio::select! {
            #[cfg(all())]
            inner = future::ready(5usize) => outer + inner,
        },
    };
    assert_eq!(nested, 9);
}
