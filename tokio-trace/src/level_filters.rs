//! Trace verbosity level filtering.
//!
//! # Compile time filters
//!
//! Trace verbosity levels can be statically disabled at compile time via Cargo
//! features, similar to the [`log` crate]. Trace instrumentation at disabled
//! levels will be skipped and will not even be present in the resulting binary
//! unless the verbosity level is specified dynamically. This level is
//! configured separately for release and debug builds. The features are:
//!
//! * `max_level_off`
//! * `max_level_error`
//! * `max_level_warn`
//! * `max_level_info`
//! * `max_level_debug`
//! * `max_level_trace`
//! * `release_max_level_off`
//! * `release_max_level_error`
//! * `release_max_level_warn`
//! * `release_max_level_info`
//! * `release_max_level_debug`
//! * `release_max_level_trace`
//!
//! These features control the value of the `STATIC_MAX_LEVEL` constant. The
//! instrumentation macros macros check this value before recording an event or
//! constructing a span. By default, no levels are disabled.
//!
//! For example, a crate can disable trace level instrumentation in debug builds
//! and trace, debug, and info level instrumentation in release builds with the
//! following configuration:
//!
//! ```toml
//! [dependencies]
//! tokio-trace = { version = "0.1", features = ["max_level_debug", "release_max_level_warn"] }
//! ```
//!
//! [`log` crate]: https://docs.rs/log/0.4.6/log/#compile-time-filters
use std::cmp::Ordering;
use tokio_trace_core::Level;

/// A filter comparable to trace verbosity `Level`.
///
/// If a `Level` is considered less than a `LevelFilter`, it should be
/// considered disabled; if greater than or equal to the `LevelFilter`, that
/// level is enabled.
///
/// Note that this is essentially identical to the `Level` type, but with the
/// addition of an `OFF` level that completely disables all trace
/// instrumentation.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct LevelFilter(Option<Level>);

impl LevelFilter {
    /// The "off" level.
    ///
    /// Designates that trace instrumentation should be completely disabled.
    pub const OFF: LevelFilter = LevelFilter(None);
    /// The "error" level.
    ///
    /// Designates very serious errors.
    pub const ERROR: LevelFilter = LevelFilter(Some(Level::ERROR));
    /// The "warn" level.
    ///
    /// Designates hazardous situations.
    pub const WARN: LevelFilter = LevelFilter(Some(Level::WARN));
    /// The "info" level.
    ///
    /// Designates useful information.
    pub const INFO: LevelFilter = LevelFilter(Some(Level::INFO));
    /// The "debug" level.
    ///
    /// Designates lower priority information.
    pub const DEBUG: LevelFilter = LevelFilter(Some(Level::DEBUG));
    /// The "trace" level.
    ///
    /// Designates very low priority, often extremely verbose, information.
    pub const TRACE: LevelFilter = LevelFilter(Some(Level::TRACE));
}

impl PartialEq<LevelFilter> for Level {
    fn eq(&self, other: &LevelFilter) -> bool {
        match other.0 {
            None => false,
            Some(ref level) => self.eq(level),
        }
    }
}

impl PartialOrd<LevelFilter> for Level {
    fn partial_cmp(&self, other: &LevelFilter) -> Option<Ordering> {
        match other.0 {
            None => Some(Ordering::Less),
            Some(ref level) => self.partial_cmp(level),
        }
    }
}

/// The statically configured maximum trace level.
///
/// See the [module-level documentation] for information on how to configure
/// this.
///
/// This value is checked by the `event!` and `span!` macros. Code that
/// manually constructs events or spans via the `Event::record` function or
/// `Span` constructors should compare the level against this value to
/// determine if those spans or events are enabled.
///
/// [module-level documentation]: ../index.html#compile-time-filters
pub const STATIC_MAX_LEVEL: LevelFilter = MAX_LEVEL;

cfg_if! {
    if #[cfg(all(not(debug_assertions), feature = "release_max_level_off"))] {
        const MAX_LEVEL: LevelFilter = LevelFilter::OFF;
    } else if #[cfg(all(not(debug_assertions), feature = "release_max_level_error"))] {
        const MAX_LEVEL: LevelFilter = LevelFilter::ERROR;
    } else if #[cfg(all(not(debug_assertions), feature = "release_max_level_warn"))] {
        const MAX_LEVEL: LevelFilter = LevelFilter::WARN;
    } else if #[cfg(all(not(debug_assertions), feature = "release_max_level_info"))] {
        const MAX_LEVEL: LevelFilter = LevelFilter::INFO;
    } else if #[cfg(all(not(debug_assertions), feature = "release_max_level_debug"))] {
        const MAX_LEVEL: LevelFilter = LevelFilter::DEBUG;
    } else if #[cfg(all(not(debug_assertions), feature = "release_max_level_trace"))] {
        const MAX_LEVEL: LevelFilter = LevelFilter::TRACE;
    } else if #[cfg(feature = "max_level_off")] {
        const MAX_LEVEL: LevelFilter = LevelFilter::OFF;
    } else if #[cfg(feature = "max_level_error")] {
        const MAX_LEVEL: LevelFilter = LevelFilter::ERROR;
    } else if #[cfg(feature = "max_level_warn")] {
        const MAX_LEVEL: LevelFilter = LevelFilter::WARN;
    } else if #[cfg(feature = "max_level_info")] {
        const MAX_LEVEL: LevelFilter = LevelFilter::INFO;
    } else if #[cfg(feature = "max_level_debug")] {
        const MAX_LEVEL: LevelFilter = LevelFilter::DEBUG;
    } else {
        const MAX_LEVEL: LevelFilter = LevelFilter::TRACE;
    }
}
