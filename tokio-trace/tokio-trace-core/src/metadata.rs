//! Metadata describing trace data.
use super::{
    callsite::{self, Callsite},
    field,
};
use std::fmt;

/// Metadata describing a [span] or [event].
///
/// This includes the source code location where the span occurred, the names of
/// its fields, et cetera.
///
/// Metadata is used by [`Subscriber`]s when filtering spans and events, and it
/// may also be used as part of their data payload.
///
/// When created by the `event!` or `span!` macro, the metadata describing a
/// particular event or span is constructed statically and exists as a single
/// static instance. Thus, the overhead of creating the metadata is
/// _significantly_ lower than that of creating the actual span. Therefore,
/// filtering is based on metadata, rather than  on the constructed span.
///
/// **Note**: Although instances of `Metadata` cannot be compared directly, they
/// provide a method [`Metadata::id()`] which returns an an opaque [callsite
/// identifier] which uniquely identifies the callsite where the metadata
/// originated. This can be used for determining if two Metadata correspond to
/// the same callsite.
///
/// [span]: ../span
/// [`Subscriber`]: ../subscriber/trait.Subscriber.html
/// [`Metadata::id()`]: struct.Metadata.html#method.id
/// [callsite identifier]: ../callsite/struct.Identifier.html
// TODO: When `const fn` is stable, make this type's fields private.
pub struct Metadata<'a> {
    /// The name of the span described by this metadata.
    ///
    /// **Warning**: The fields on this type are currently `pub` because it must
    /// be able to be constructed statically by macros. However, when `const
    /// fn`s are available on stable Rust, this will no longer be necessary.
    /// Thus, these fields are *not* considered stable public API, and they may
    /// change warning. Do not rely on any fields on `Metadata`. When
    /// constructing new `Metadata`, use the `metadata!` macro or the
    /// `Metadata::new` constructor instead!
    #[doc(hidden)]
    pub name: &'static str,

    /// The part of the system that the span that this metadata describes
    /// occurred in.
    ///
    /// Typically, this is the module path, but alternate targets may be set
    /// when spans or events are constructed.
    ///
    /// **Warning**: The fields on this type are currently `pub` because it must
    /// be able to be constructed statically by macros. However, when `const
    /// fn`s are available on stable Rust, this will no longer be necessary.
    /// Thus, these fields are *not* considered stable public API, and they may
    /// change warning. Do not rely on any fields on `Metadata`. When
    /// constructing new `Metadata`, use the `metadata!` macro or the
    /// `Metadata::new` constructor instead!
    #[doc(hidden)]
    pub target: &'a str,

    /// The level of verbosity of the described span.
    ///
    /// **Warning**: The fields on this type are currently `pub` because it must
    /// be able to be constructed statically by macros. However, when `const
    /// fn`s are available on stable Rust, this will no longer be necessary.
    /// Thus, these fields are *not* considered stable public API, and they may
    /// change warning. Do not rely on any fields on `Metadata`. When
    /// constructing new `Metadata`, use the `metadata!` macro or the
    /// `Metadata::new` constructor instead!
    #[doc(hidden)]
    pub level: Level,

    /// The name of the Rust module where the span occurred, or `None` if this
    /// could not be determined.
    ///
    /// **Warning**: The fields on this type are currently `pub` because it must
    /// be able to be constructed statically by macros. However, when `const
    /// fn`s are available on stable Rust, this will no longer be necessary.
    /// Thus, these fields are *not* considered stable public API, and they may
    /// change warning. Do not rely on any fields on `Metadata`. When
    /// constructing new `Metadata`, use the `metadata!` macro or the
    /// `Metadata::new` constructor instead!
    #[doc(hidden)]
    pub module_path: Option<&'a str>,

    /// The name of the source code file where the span occurred, or `None` if
    /// this could not be determined.
    ///
    /// **Warning**: The fields on this type are currently `pub` because it must
    /// be able to be constructed statically by macros. However, when `const
    /// fn`s are available on stable Rust, this will no longer be necessary.
    /// Thus, these fields are *not* considered stable public API, and they may
    /// change warning. Do not rely on any fields on `Metadata`. When
    /// constructing new `Metadata`, use the `metadata!` macro or the
    /// `Metadata::new` constructor instead!
    #[doc(hidden)]
    pub file: Option<&'a str>,

    /// The line number in the source code file where the span occurred, or
    /// `None` if this could not be determined.
    ///
    /// **Warning**: The fields on this type are currently `pub` because it must
    /// be able to be constructed statically by macros. However, when `const
    /// fn`s are available on stable Rust, this will no longer be necessary.
    /// Thus, these fields are *not* considered stable public API, and they may
    /// change warning. Do not rely on any fields on `Metadata`. When
    /// constructing new `Metadata`, use the `metadata!` macro or the
    /// `Metadata::new` constructor instead!
    #[doc(hidden)]
    pub line: Option<u32>,

    /// The names of the key-value fields attached to the described span or
    /// event.
    ///
    /// **Warning**: The fields on this type are currently `pub` because it must
    /// be able to be constructed statically by macros. However, when `const
    /// fn`s are available on stable Rust, this will no longer be necessary.
    /// Thus, these fields are *not* considered stable public API, and they may
    /// change warning. Do not rely on any fields on `Metadata`. When
    /// constructing new `Metadata`, use the `metadata!` macro or the
    /// `Metadata::new` constructor instead!
    #[doc(hidden)]
    pub fields: field::FieldSet,
}

/// Describes the level of verbosity of a span or event.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Level(LevelInner);

// ===== impl Metadata =====

impl<'a> Metadata<'a> {
    /// Construct new metadata for a span, with a name, target, level, field
    /// names, and optional source code location.
    pub fn new(
        name: &'static str,
        target: &'a str,
        level: Level,
        module_path: Option<&'a str>,
        file: Option<&'a str>,
        line: Option<u32>,
        field_names: &'static [&'static str],
        callsite: &'static Callsite,
    ) -> Self {
        Metadata {
            name,
            target,
            level,
            module_path,
            file,
            line,
            fields: field::FieldSet {
                names: field_names,
                callsite: callsite::Identifier(callsite),
            },
        }
    }

    /// Returns the set of fields on the described span.
    pub fn fields(&self) -> &field::FieldSet {
        &self.fields
    }

    /// Returns the level of verbosity of the described span.
    pub fn level(&self) -> &Level {
        &self.level
    }

    /// Returns the name of the span.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Returns a string describing the part of the system where the span or
    /// event that this metadata describes occurred.
    ///
    /// Typically, this is the module path, but alternate targets may be set
    /// when spans or events are constructed.
    pub fn target(&self) -> &'a str {
        self.target
    }

    /// Returns the path to the Rust module where the span occurred, or
    /// `None` if the module path is unknown.
    pub fn module_path(&self) -> Option<&'a str> {
        self.module_path
    }

    /// Returns the name of the source code file where the span
    /// occurred, or `None` if the file is unknown
    pub fn file(&self) -> Option<&'a str> {
        self.file
    }

    /// Returns the line number in the source code file where the span
    /// occurred, or `None` if the line number is unknown.
    pub fn line(&self) -> Option<u32> {
        self.line
    }

    /// Returns an opaque `Identifier` that uniquely identifies the callsite
    /// this `Metadata` originated from.
    #[inline]
    pub fn callsite(&self) -> callsite::Identifier {
        self.fields.callsite()
    }
}

impl<'a> fmt::Debug for Metadata<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Metadata")
            .field("name", &self.name)
            .field("target", &self.target)
            .field("level", &self.level)
            .field("module_path", &self.module_path)
            .field("file", &self.file)
            .field("line", &self.line)
            .field("field_names", &self.fields)
            .finish()
    }
}

// ===== impl Level =====

impl Level {
    /// The "error" level.
    ///
    /// Designates very serious errors.
    pub const ERROR: Level = Level(LevelInner::Error);
    /// The "warn" level.
    ///
    /// Designates hazardous situations.
    pub const WARN: Level = Level(LevelInner::Warn);
    /// The "info" level.
    ///
    /// Designates useful information.
    pub const INFO: Level = Level(LevelInner::Info);
    /// The "debug" level.
    ///
    /// Designates lower priority information.
    pub const DEBUG: Level = Level(LevelInner::Debug);
    /// The "trace" level.
    ///
    /// Designates very low priority, often extremely verbose, information.
    pub const TRACE: Level = Level(LevelInner::Trace);
}

#[repr(usize)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
enum LevelInner {
    /// The "error" level.
    ///
    /// Designates very serious errors.
    Error = 1,
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
