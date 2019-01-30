use std::fmt;
use tokio_trace::Metadata;

#[derive(Debug, Eq, PartialEq, Default)]
pub struct Expected {
    pub name: Option<String>,
    pub level: Option<tokio_trace::Level>,
    pub target: Option<String>,
}

impl Expected {
    pub(in support) fn check(&self, actual: &Metadata, ctx: fmt::Arguments) {
        if let Some(ref expected_name) = self.name {
            let name = actual.name();
            assert!(
                expected_name == name,
                "expected {} to be named `{}`, but got one named `{}`",
                ctx, expected_name, name
            )
        }

        if let Some(ref expected_level) = self.level {
            let level = actual.level();
            assert!(
                expected_level == level,
                "expected {} to be at level `{:?}`, but it was at level `{:?}` instead",
                ctx, expected_level, level,
            )
        }

        if let Some(ref expected_target) = self.target {
            let target = actual.target();
            assert!(
                expected_target == &target,
                "expected {} to have target `{}`, but it had target `{}` instead",
                ctx, expected_target, target,
            )
        }
    }
}
