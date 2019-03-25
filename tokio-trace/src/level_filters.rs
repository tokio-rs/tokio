use std::cmp::Ordering;
use tokio_trace_core::Level;

/// `LevelFilter` is used to statistically filter the logging messages based on its `Level`.
/// Logging messages will be discarded if its `Level` is greater than `LevelFilter`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct LevelFilter(Option<Level>);

impl LevelFilter {
    /// The "off" level.
    ///
    /// Designates that logging should be to turned off.
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

/// The statically resolved maximum trace level.
///
/// See the crate level documentation for information on how to configure this.
///
/// This value is checked by the `event` macro. Code that manually calls functions on that value
/// should compare the level against this value.
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
