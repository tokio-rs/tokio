#[macro_use]
extern crate tokio_trace_core;
use tokio_trace_core::{
    callsite::Callsite,
    metadata::{Kind, Level, Metadata},
    subscriber::Interest,
};

#[test]
fn metadata_macro_api() {
    // This test should catch any inadvertant breaking changes
    // caused bu changes to the macro.
    struct TestCallsite;

    impl Callsite for TestCallsite {
        fn set_interest(&self, _: Interest) {
            unimplemented!("test")
        }
        fn metadata(&self) -> &Metadata {
            unimplemented!("test")
        }
    }

    static CALLSITE: TestCallsite = TestCallsite;
    let _metadata = metadata! {
        name: "test_metadata",
        target: "test_target",
        level: Level::DEBUG,
        fields: &["foo", "bar", "baz"],
        callsite: &CALLSITE,
        kind: Kind::SPAN,
    };
    let _metadata = metadata! {
        name: "test_metadata",
        target: "test_target",
        level: Level::TRACE,
        fields: &[],
        callsite: &CALLSITE,
        kind: Kind::EVENT,
    };
    let _metadata = metadata! {
        name: "test_metadata",
        target: "test_target",
        level: Level::INFO,
        fields: &[],
        callsite: &CALLSITE,
        kind: Kind::EVENT
    };
}
