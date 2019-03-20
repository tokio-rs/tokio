use std::cmp::Ordering;
use tokio_trace_core::Level;

/// Describes the level of verbosity of a span or event.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct LevelFilter(LevelFilterInner, Option<Level>);

impl LevelFilter {
    /// The "zero" level.
    ///
    /// Designates no logging.
    pub const ZERO: LevelFilter = LevelFilter(LevelFilterInner::Zero, None);
    /// The "error" level.
    ///
    /// Designates very serious errors.
    pub const ERROR: LevelFilter = LevelFilter(LevelFilterInner::Error, Some(Level::ERROR));
    /// The "warn" level.
    ///
    /// Designates hazardous situations.
    pub const WARN: LevelFilter = LevelFilter(LevelFilterInner::Warn, Some(Level::WARN));
    /// The "info" level.
    ///
    /// Designates useful information.
    pub const INFO: LevelFilter = LevelFilter(LevelFilterInner::Info, Some(Level::INFO));
    /// The "debug" level.
    ///
    /// Designates lower priority information.
    pub const DEBUG: LevelFilter = LevelFilter(LevelFilterInner::Debug, Some(Level::DEBUG));
    /// The "trace" level.
    ///
    /// Designates very low priority, often extremely verbose, information.
    pub const TRACE: LevelFilter = LevelFilter(LevelFilterInner::Trace, Some(Level::TRACE));
}

#[repr(usize)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
enum LevelFilterInner {
    Zero = 0,
    /// The "error" level.
    ///
    /// Designates very serious errors.
    Error,
    /// The "warn" level.
    ///
    /// Designates hazardous situations.
    Warn,
    /// The "info" level.
    ///
    /// Designates useful information.
    Info,
    /// The "debug" level.
    ///
    /// Designates lower priority information.
    Debug,
    /// The "trace" level.
    ///
    /// Designates very low priority, often extremely verbose, information.
    Trace,
}

impl PartialEq<LevelFilter> for Level {
    fn eq(&self, other: &LevelFilter) -> bool {
        match other.1 {
            None => false,
            Some(ref level) => self.eq(level),
        }
    }
}

impl PartialOrd<LevelFilter> for Level {
    fn partial_cmp(&self, other: &LevelFilter) -> Option<Ordering> {
        match other.1 {
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
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::ZERO;

#[cfg(feature = "max_level_zero")]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::ZERO;

#[cfg(feature = "max_level_error")]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::ERROR;

#[cfg(feature = "max_level_warn")]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::WARN;

#[cfg(feature = "max_level_info")]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::INFO;

#[cfg(feature = "max_level_debug")]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::DEBUG;

#[cfg(feature = "max_level_trace")]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::TRACE;

#[cfg(all(not(debug_assertions), feature = "release_max_level_zero"))]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::ZERO;

#[cfg(all(not(debug_assertions), feature = "release_max_level_error"))]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::ERROR;

#[cfg(all(not(debug_assertions), feature = "release_max_level_warn"))]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::WARN;

#[cfg(all(not(debug_assertions), feature = "release_max_level_info"))]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::INFO;

#[cfg(all(not(debug_assertions), feature = "release_max_level_debug"))]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::DEBUG;

#[cfg(all(not(debug_assertions), feature = "release_max_level_trace"))]
pub const STATIC_MAX_LEVEL: LevelFilter = LevelFilter::TRACE;
