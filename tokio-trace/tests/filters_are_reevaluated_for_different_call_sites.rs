// Tests that depend on a count of the number of times their filter is evaluated
// cant exist in the same file with other tests that add subscribers to the
// registry. The registry was changed so that each time a new dispatcher is
// added all filters are re-evaluated. The tests being run only in separate
// threads with shared global state lets them interfere with eachother

#[macro_use]
extern crate tokio_trace;
mod support;

use self::support::*;
use tokio_trace::subscriber::with_default;
use tokio_trace::Level;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[test]
fn filters_are_reevaluated_for_different_call_sites() {
    // Asserts that the `span!` macro caches the result of calling
    // `Subscriber::enabled` for each span.
    let charlie_count = Arc::new(AtomicUsize::new(0));
    let dave_count = Arc::new(AtomicUsize::new(0));
    let charlie_count2 = charlie_count.clone();
    let dave_count2 = dave_count.clone();

    let subscriber = subscriber::mock()
        .with_filter(move |meta| {
            println!("Filter: {:?}", meta.name);
            match meta.name {
                "charlie" => {
                    charlie_count2.fetch_add(1, Ordering::Relaxed);
                    false
                }
                "dave" => {
                    dave_count2.fetch_add(1, Ordering::Relaxed);
                    true
                }
                _ => false,
            }
        })
        .run();

    with_default(subscriber, move || {
        // Enter "charlie" and then "dave". The dispatcher expects to see "dave" but
        // not "charlie."
        let charlie = span!(Level::TRACE, "charlie");
        let dave = charlie.in_scope(|| {
            let dave = span!(Level::TRACE, "dave");
            dave.in_scope(|| {});
            dave
        });

        // The filter should have seen each span a single time.
        assert_eq!(charlie_count.load(Ordering::Relaxed), 1);
        assert_eq!(dave_count.load(Ordering::Relaxed), 1);

        charlie.in_scope(|| dave.in_scope(|| {}));

        // The subscriber should see "dave" again, but the filter should not have
        // been called.
        assert_eq!(charlie_count.load(Ordering::Relaxed), 1);
        assert_eq!(dave_count.load(Ordering::Relaxed), 1);

        // A different span with the same name has a different call site, so it
        // should cause the filter to be reapplied.
        let charlie2 = span!(Level::TRACE, "charlie");
        charlie.in_scope(|| {});
        assert_eq!(charlie_count.load(Ordering::Relaxed), 2);
        assert_eq!(dave_count.load(Ordering::Relaxed), 1);

        // But, the filter should not be re-evaluated for the new "charlie" span
        // when it is re-entered.
        charlie2.in_scope(|| span!(Level::TRACE, "dave").in_scope(|| {}));
        assert_eq!(charlie_count.load(Ordering::Relaxed), 2);
        assert_eq!(dave_count.load(Ordering::Relaxed), 2);
    });
}
