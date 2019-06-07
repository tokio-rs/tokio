/// Constructs a new span.
///
/// # Examples
///
/// Creating a new span with no fields:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// let span = span!(Level::TRACE, "my span");
/// span.in_scope(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
///
/// Creating a span with custom target:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// span!(Level::TRACE, target: "app_span", "my span");
/// # }
/// ```
///
/// # Fields
///
/// Creating a span with fields:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// span!(Level::TRACE, "my span", foo = 2, bar = "a string").in_scope(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
///
/// Note that a trailing comma on the final field is valid:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// span!(
///     Level::TRACE,
///     "my span",
///     foo = 2,
///     bar = "a string",
/// );
/// # }
/// ```
///
/// As shorthand, local variables may be used as field values without an
/// assignment, similar to [struct initializers]. For example:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// let user = "ferris";
///
/// span!(Level::TRACE, "login", user);
/// // is equivalent to:
/// span!(Level::TRACE, "login", user = user);
/// # }
///```
///
/// Field names can include dots:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// let user = "ferris";
/// let email = "ferris@rust-lang.org";
/// span!(Level::TRACE, "login", user, user.email = email);
/// # }
///```
///
/// Since field names can include dots, fields on local structs can be used
/// using the local variable shorthand:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// # struct User {
/// #    name: &'static str,
/// #    email: &'static str,
/// # }
/// let user = User {
///     name: "ferris",
///     email: "ferris@rust-lang.org",
/// };
/// // the span will have the fields `user.name = "ferris"` and
/// // `user.email = "ferris@rust-lang.org"`.
/// span!(Level::TRACE, "login", user.name, user.email);
/// # }
///```
///
// TODO(#1138): determine a new syntax for uninitialized span fields, and
// re-enable this.
// /// Field values may be recorded after the span is created. The `_` character is
// /// used to represent a field whose value has yet to be recorded:
// /// ```
// /// # #[macro_use]
// /// # extern crate tokio_trace;
// /// # use tokio_trace::Level;
// /// # fn main() {
// /// let my_span = span!(Level::TRACE, "my span", foo = 2, bar = _);
// /// my_span.record("bar", &7);
// /// # }
// /// ```
// ///
/// The `?` sigil is shorthand for `field::debug`:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// #[derive(Debug)]
/// struct MyStruct {
///     field: &'static str,
/// }
///
/// let my_struct = MyStruct {
///     field: "Hello world!"
/// };
///
/// // `my_struct` will be recorded using its `fmt::Debug` implementation.
/// let my_span = span!(Level::TRACE, "my span", foo = ?my_struct);
/// # }
/// ```
///
/// The `%` character is shorthand for `field::display`:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// # #[derive(Debug)]
/// # struct MyStruct {
/// #     field: &'static str,
/// # }
/// #
/// # let my_struct = MyStruct {
/// #     field: "Hello world!"
/// # };
/// // `my_struct.field` will be recorded using its `fmt::Display` implementation.
/// let my_span = span!(Level::TRACE, "my span", foo = %my_struct.field);
/// # }
/// ```
///
/// The `display` and `debug` sigils may also be used with local variable shorthand:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// # #[derive(Debug)]
/// # struct MyStruct {
/// #     field: &'static str,
/// # }
/// #
/// # let my_struct = MyStruct {
/// #     field: "Hello world!"
/// # };
/// // `my_struct.field` will be recorded using its `fmt::Display` implementation.
/// let my_span = span!(Level::TRACE, "my span", %my_struct.field);
/// # }
/// ```
///
/// Note that a span may have up to 32 fields. The following will not compile:
/// ```rust,compile_fail
///  # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// span!(
///     Level::TRACE,
///     "too many fields!",
///     a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9,
///     j = 10, k = 11, l = 12, m = 13, n = 14, o = 15, p = 16, q = 17,
///     r = 18, s = 19, t = 20, u = 21, v = 22, w = 23, x = 24, y = 25,
///     z = 26, aa = 27, bb = 28, cc = 29, dd = 30, ee = 31, ff = 32, gg = 33
/// );
/// # }
/// ```
/// [struct initializers]: https://doc.rust-lang.org/book/ch05-01-defining-structs.html#using-the-field-init-shorthand-when-variables-and-fields-have-the-same-name
#[macro_export(local_inner_macros)]
macro_rules! span {
    ($lvl:expr, target: $target:expr, parent: $parent:expr, $name:expr) => {
        span!($lvl, target: $target, parent: $parent, $name,)
    };
    ($lvl:expr, target: $target:expr, parent: $parent:expr, $name:expr, $($fields:tt)*) => {
        {
            use $crate::callsite;
            use $crate::callsite::Callsite;
            let callsite = callsite! {
                name: $name,
                kind: $crate::metadata::Kind::SPAN,
                target: $target,
                level: $lvl,
                fields: $($fields)*
            };
            let meta = callsite.metadata();

            if $lvl <= $crate::level_filters::STATIC_MAX_LEVEL && is_enabled!(callsite) {
                $crate::Span::child_of(
                    $parent,
                    meta,
                    &valueset!(meta.fields(), $($fields)*),
                )
            } else {
                 __tokio_trace_disabled_span!(
                    meta,
                    &valueset!(meta.fields(), $($fields)*)
                )
            }
        }
    };
    ($lvl:expr, target: $target:expr, $name:expr, $($fields:tt)*) => {
        {
            use $crate::callsite;
            use $crate::callsite::Callsite;
            let callsite = callsite! {
                name: $name,
                kind: $crate::metadata::Kind::SPAN,
                target: $target,
                level: $lvl,
                fields: $($fields)*
            };
            let meta = callsite.metadata();

            if $lvl <= $crate::level_filters::STATIC_MAX_LEVEL && is_enabled!(callsite) {
                $crate::Span::new(
                    meta,
                    &valueset!(meta.fields(), $($fields)*)
                )
            } else {
                __tokio_trace_disabled_span!(
                    meta,
                    &valueset!(meta.fields(), $($fields)*)
                )
            }
        }

    };
    ($lvl:expr, target: $target:expr, parent: $parent:expr, $name:expr) => {
        span!($lvl, target: $target, parent: $parent, $name,)
    };
    ($lvl:expr, parent: $parent:expr, $name:expr, $($fields:tt)*) => {
        span!(
            $lvl,
            target: __tokio_trace_module_path!(),
            parent: $parent,
            $name,
            $($fields)*
        )
    };
    ($lvl:expr, parent: $parent:expr, $name:expr) => {
        span!(
            $lvl,
            target: __tokio_trace_module_path!(),
            parent: $parent,
            $name,
        )
    };
    ($lvl:expr, target: $target:expr, $name:expr, $($fields:tt)*) => {
        span!(
            $lvl,
            target: $target,
            $name,
            $($fields)*
        )
    };
    ($lvl:expr, target: $target:expr, $name:expr) => {
        span!($lvl, target: $target, $name,)
    };
    ($lvl:expr, $name:expr, $($fields:tt)*) => {
        span!(
            $lvl,
            target: __tokio_trace_module_path!(),
            $name,
            $($fields)*
        )
    };
    ($lvl:expr, $name:expr) => {
        span!(
            $lvl,
            target: __tokio_trace_module_path!(),
            $name,
        )
    };
}

/// Constructs a span at the trace level.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// let span = trace_span!("my span");
/// span.in_scope(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! trace_span {
    (target: $target:expr, parent: $parent:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::TRACE,
            target: $target,
            parent: $parent,
            $name,
            $($field)*
        )
    };
    (target: $target:expr, parent: $parent:expr, $name:expr) => {
        trace_span!(target: $target, parent: $parent, $name,)
    };
    (parent: $parent:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::TRACE,
            target: __tokio_trace_module_path!(),
            $name,
            $($field)*
        )
    };
    (parent: $parent:expr, $name:expr) => {
        trace_span!(parent: $parent, $name,)
    };
    (target: $target:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::TRACE,
            target: $target,
            $name,
            $($field)*
        )
    };
    (target: $target:expr, $name:expr) => {
        trace_span!(target: $target, $name,)
    };
    ($name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::TRACE,
            target: __tokio_trace_module_path!(),
            $name,
            $($field)*
        )
    };
    ($name:expr) => {trace_span!($name,)};
}

/// Constructs a span at the debug level.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// let span = debug_span!("my span");
/// span.in_scope(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! debug_span {
    (target: $target:expr, parent: $parent:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::INFO,
            target: $target,
            parent: $parent,
            $name,
            $($field)*
        )
    };
    (target: $target:expr, parent: $parent:expr, $name:expr) => {
        debug_span!(target: $target, parent: $parent, $name,)
    };
    (parent: $parent:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::INFO,
            target: __tokio_trace_module_path!(),
            $name,
            $($field)*
        )
    };
    (parent: $parent:expr, $name:expr) => {
        debug_span!(parent: $parent, $name,)
    };
    (target: $target:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::INFO,
            target: $target,
            $name,
            $($field)*
        )
    };
    (target: $target:expr, $name:expr) => {
        debug_span!(target: $target, $name,)
    };
    ($name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::INFO,
            target: __tokio_trace_module_path!(),
            $name,
            $($field)*
        )
    };
    ($name:expr) => {debug_span!($name,)};
}

/// Constructs a span at the info level.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// let span = info_span!("my span");
/// span.in_scope(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! info_span {
    (target: $target:expr, parent: $parent:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::INFO,
            target: $target,
            parent: $parent,
            $name,
            $($field)*
        )
    };
    (target: $target:expr, parent: $parent:expr, $name:expr) => {
        info_span!(target: $target, parent: $parent, $name,)
    };
    (parent: $parent:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::INFO,
            target: __tokio_trace_module_path!(),
            $name,
            $($field)*
        )
    };
    (parent: $parent:expr, $name:expr) => {
        info_span!(parent: $parent, $name,)
    };
    (target: $target:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::INFO,
            target: $target,
            $name,
            $($field)*
        )
    };
    (target: $target:expr, $name:expr) => {
        info_span!(target: $target, $name,)
    };
    ($name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::INFO,
            target: __tokio_trace_module_path!(),
            $name,
            $($field)*
        )
    };
    ($name:expr) => {info_span!($name,)};
}

/// Constructs a span at the warn level.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// let span = warn_span!("my span");
/// span.in_scope(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! warn_span {
    (target: $target:expr, parent: $parent:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::WARN,
            target: $target,
            parent: $parent,
            $name,
            $($field)*
        )
    };
    (target: $target:expr, parent: $parent:expr, $name:expr) => {
        warn_span!(target: $target, parent: $parent, $name,)
    };
    (parent: $parent:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::WARN,
            target: __tokio_trace_module_path!(),
            $name,
            $($field)*
        )
    };
    (parent: $parent:expr, $name:expr) => {
        warn_span!(parent: $parent, $name,)
    };
    (target: $target:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::WARN,
            target: $target,
            $name,
            $($field)*
        )
    };
    (target: $target:expr, $name:expr) => {
        warn_span!(target: $target, $name,)
    };
    ($name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::WARN,
            target: __tokio_trace_module_path!(),
            $name,
            $($field)*
        )
    };
    ($name:expr) => {warn_span!($name,)};
}
/// Constructs a span at the error level.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// let span = error_span!("my span");
/// span.in_scope(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! error_span {
    (target: $target:expr, parent: $parent:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::ERROR,
            target: $target,
            parent: $parent,
            $name,
            $($field)*
        )
    };
    (target: $target:expr, parent: $parent:expr, $name:expr) => {
        error_span!(target: $target, parent: $parent, $name,)
    };
    (parent: $parent:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::ERROR,
            target: __tokio_trace_module_path!(),
            $name,
            $($field)*
        )
    };
    (parent: $parent:expr, $name:expr) => {
        error_span!(parent: $parent, $name,)
    };
    (target: $target:expr, $name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::ERROR,
            target: $target,
            $name,
            $($field)*
        )
    };
    (target: $target:expr, $name:expr) => {
        error_span!(target: $target, $name,)
    };
    ($name:expr, $($field:tt)*) => {
        span!(
            $crate::Level::ERROR,
            target: __tokio_trace_module_path!(),
            $name,
            $($field)*
        )
    };
    ($name:expr) => {error_span!($name,)};
}

/// Constructs a new `Event`.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// use tokio_trace::{Level, field};
///
/// # fn main() {
/// let data = (42, "fourty-two");
/// let private_data = "private";
/// let error = "a bad error";
///
/// event!(Level::ERROR, { error = field::display(error) }, "Received error");
/// event!(target: "app_events", Level::WARN, {
///         private_data = private_data,
///         data = field::debug(data),
///     },
///     "App warning: {}", error
/// );
/// event!(Level::INFO, the_answer = data.0);
/// # }
/// ```
///
/// Note that *unlike `span!`*, `event!` requires a value for all fields. As
/// events are recorded immediately when the macro is invoked, there is no
/// opportunity for fields to be recorded later. A trailing comma on the final
/// field is valid.
///
/// For example, the following does not compile:
/// ```rust,compile_fail
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// event!(Level::Info, foo = 5, bad_field, bar = "hello")
/// #}
/// ```
/// Shorthand for `field::debug`:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// #[derive(Debug)]
/// struct MyStruct {
///     field: &'static str,
/// }
///
/// let my_struct = MyStruct {
///     field: "Hello world!"
/// };
///
/// // `my_struct` will be recorded using its `fmt::Debug` implementation.
/// event!(Level::TRACE, my_struct = ?my_struct);
/// # }
/// ```
/// Shorthand for `field::display`:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// # #[derive(Debug)]
/// # struct MyStruct {
/// #     field: &'static str,
/// # }
/// #
/// # let my_struct = MyStruct {
/// #     field: "Hello world!"
/// # };
/// // `my_struct.field` will be recorded using its `fmt::Display` implementation.
/// event!(Level::TRACE, my_struct.field = %my_struct.field);
/// # }
/// ```
/// Events may have up to 32 fields. The following will not compile:
/// ```rust,compile_fail
///  # #[macro_use]
/// # extern crate tokio_trace;
/// # use tokio_trace::Level;
/// # fn main() {
/// event!(Level::INFO,
///     a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9,
///     j = 10, k = 11, l = 12, m = 13, n = 14, o = 15, p = 16, q = 17,
///     r = 18, s = 19, t = 20, u = 21, v = 22, w = 23, x = 24, y = 25,
///     z = 26, aa = 27, bb = 28, cc = 29, dd = 30, ee = 31, ff = 32, gg = 33
/// );
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! event {
    (target: $target:expr, $lvl:expr, { $($fields:tt)* } )=> ({
        {
            __tokio_trace_log!(
                target: $target,
                $lvl,
                $($fields)*
            );

            if $lvl <= $crate::level_filters::STATIC_MAX_LEVEL {
                #[allow(unused_imports)]
                use $crate::{callsite, dispatcher, Event, field::{Value, ValueSet}};
                use $crate::callsite::Callsite;
                let callsite = callsite! {
                    name: __tokio_trace_concat!(
                        "event ",
                        __tokio_trace_file!(),
                        ":",
                        __tokio_trace_line!()
                    ),
                    kind: $crate::metadata::Kind::EVENT,
                    target: $target,
                    level: $lvl,
                    fields: $($fields)*
                };
                if is_enabled!(callsite) {
                    let meta = callsite.metadata();
                    Event::dispatch(meta, &valueset!(meta.fields(), $($fields)*) );
                }
            }
        }
    });
    (target: $target:expr, $lvl:expr, { $($fields:tt)* }, $($arg:tt)+ ) => ({
        event!(
            target: $target,
            $lvl,
            { message = __tokio_trace_format_args!($($arg)+), $($fields)* }
        )
    });
    (target: $target:expr, $lvl:expr, $($k:ident).+ = $($fields:tt)* ) => (
        event!(target: $target, $lvl, { $($k).+ = $($fields)* })
    );
    (target: $target:expr, $lvl:expr, $($arg:tt)+ ) => (
        event!(target: $target, $lvl, { }, $($arg)+)
    );
    ( $lvl:expr, { $($fields:tt)* }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $lvl,
            { message = __tokio_trace_format_args!($($arg)+), $($fields)* }
        )
    );
    ( $lvl:expr, { $($fields:tt)* }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $lvl,
            { message = __tokio_trace_format_args!($($arg)+), $($fields)* }
        )
    );
    ($lvl:expr, $($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $lvl,
            { $($k).+ = $($field)*}
        )
    );
    ($lvl:expr, ?$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $lvl,
            { ?$($k).+ = $($field)*}
        )
    );
    ($lvl:expr, %$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $lvl,
            { %$($k).+ = $($field)*}
        )
    );
    ($lvl:expr, $($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $lvl,
            { $($k).+, $($field)*}
        )
    );
    ($lvl:expr, ?$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $lvl,
            { ?$($k).+, $($field)*}
        )
    );
    ($lvl:expr, %$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $lvl,
            { %$($k).+, $($field)*}
        )
    );
    ( $lvl:expr, $($arg:tt)+ ) => (
        event!(target: __tokio_trace_module_path!(), $lvl, { }, $($arg)+)
    );
}

/// Constructs an event at the trace level.
///
/// When both a message and fields are included, curly braces (`{` and `}`) are
/// used to delimit the list of fields from the format string for the message.
/// A trailing comma on the final field is valid.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use std::time::SystemTime;
/// # #[derive(Debug, Copy, Clone)] struct Position { x: f32, y: f32 }
/// # impl Position {
/// # const ORIGIN: Self = Self { x: 0.0, y: 0.0 };
/// # fn dist(&self, other: Position) -> f32 {
/// #    let x = (other.x - self.x).exp2(); let y = (self.y - other.y).exp2();
/// #    (x + y).sqrt()
/// # }
/// # }
/// # fn main() {
/// use tokio_trace::field;
///
/// let pos = Position { x: 3.234, y: -1.223 };
/// let origin_dist = pos.dist(Position::ORIGIN);
///
/// trace!(position = field::debug(pos), origin_dist = field::debug(origin_dist));
/// trace!(target: "app_events",
///         { position = field::debug(pos) },
///         "x is {} and y is {}",
///        if pos.x >= 0.0 { "positive" } else { "negative" },
///        if pos.y >= 0.0 { "positive" } else { "negative" });
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! trace {
    (target: $target:expr, { $($field:tt)* }, $($arg:tt)* ) => (
        event!(target: $target, $crate::Level::TRACE, { $($field)* }, $($arg)*)
    );
    (target: $target:expr, $($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::TRACE, { $($k).+ $($field)+ })
    );
    (target: $target:expr, ?$($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::TRACE, { $($k).+ $($field)+ })
    );
    (target: $target:expr, %$($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::TRACE, { $($k).+ $($field)+ })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::TRACE, {}, $($arg)+)
    );
    ({ $($field:tt)+ }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            { $($field)+ },
            $($arg)+
        )
    );
    ($($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            { $($k).+ = $($field)*}
        )
    );
    (?$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            { ?$($k).+ = $($field)*}
        )
    );
    (%$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            { %$($k).+ = $($field)*}
        )
    );
    ($($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            { $($k).+, $($field)*}
        )
    );
    (?$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            { ?$($k).+, $($field)*}
        )
    );
    (%$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            { %$($k).+, $($field)*}
        )
    );
    ($($arg:tt)+) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            {},
            $($arg)+
        )
    );
}

/// Constructs an event at the debug level.
///
/// When both a message and fields are included, curly braces (`{` and `}`) are
/// used to delimit the list of fields from the format string for the message.
/// A trailing comma on the final field is valid.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// # #[derive(Debug)] struct Position { x: f32, y: f32 }
/// use tokio_trace::field;
///
/// let pos = Position { x: 3.234, y: -1.223 };
///
/// debug!(?pos.x, ?pos.y);
/// debug!(target: "app_events", { position = ?pos }, "New position");
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! debug {
    (target: $target:expr, { $($field:tt)* }, $($arg:tt)* ) => (
        event!(target: $target, $crate::Level::INFO, { $($field)* }, $($arg)*)
    );
    (target: $target:expr, $($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, { $($k).+ $($field)+ })
    );
    (target: $target:expr, ?$($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, { $($k).+ $($field)+ })
    );
    (target: $target:expr, %$($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, { $($k).+ $($field)+ })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, {}, $($arg)+)
    );
    ({ $($field:tt)+ }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { $($field)+ },
            $($arg)+
        )
    );
    ($($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { $($k).+ = $($field)*}
        )
    );
    (?$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { ?$($k).+ = $($field)*}
        )
    );
    (%$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { %$($k).+ = $($field)*}
        )
    );
    ($($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { $($k).+, $($field)*}
        )
    );
    (?$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { ?$($k).+, $($field)*}
        )
    );
    (%$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { %$($k).+, $($field)*}
        )
    );
    ($($arg:tt)+) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            {},
            $($arg)+
        )
    );
}

/// Constructs an event at the info level.
///
/// When both a message and fields are included, curly braces (`{` and `}`) are
/// used to delimit the list of fields from the format string for the message.
/// A trailing comma on the final field is valid.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use std::net::Ipv4Addr;
/// # fn main() {
/// # struct Connection { port: u32,  speed: f32 }
/// use tokio_trace::field;
///
/// let addr = Ipv4Addr::new(127, 0, 0, 1);
/// let conn = Connection { port: 40, speed: 3.20 };
///
/// info!({ port = conn.port }, "connected to {}", addr);
/// info!(
///     target: "connection_events",
///     ip = %addr,
///     conn.port,
///     ?conn.speed,
/// );
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! info {
    (target: $target:expr, { $($field:tt)* }, $($arg:tt)* ) => (
        event!(target: $target, $crate::Level::INFO, { $($field)* }, $($arg)*)
    );
    (target: $target:expr, $($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, { $($k).+ $($field)+ })
    );
    (target: $target:expr, ?$($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, { $($k).+ $($field)+ })
    );
    (target: $target:expr, %$($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, { $($k).+ $($field)+ })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, {}, $($arg)+)
    );
    ({ $($field:tt)+ }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { $($field)+ },
            $($arg)+
        )
    );
    ($($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { $($k).+ = $($field)*}
        )
    );
    (?$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { ?$($k).+ = $($field)*}
        )
    );
    (%$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { %$($k).+ = $($field)*}
        )
    );
    ($($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { $($k).+, $($field)*}
        )
    );
    (?$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { ?$($k).+, $($field)*}
        )
    );
    (%$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { %$($k).+, $($field)*}
        )
    );
    ($($arg:tt)+) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            {},
            $($arg)+
        )
    );
}

/// Constructs an event at the warn level.
///
/// When both a message and fields are included, curly braces (`{` and `}`) are
/// used to delimit the list of fields from the format string for the message.
/// A trailing comma on the final field is valid.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// use tokio_trace::field;
///
/// let warn_description = "Invalid Input";
/// let input = &[0x27, 0x45];
///
/// warn!(input = field::debug(input), warning = warn_description);
/// warn!(
///     target: "input_events",
///     { warning = warn_description },
///     "Received warning for input: {:?}", input,
/// );
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! warn {
     (target: $target:expr, { $($field:tt)* }, $($arg:tt)* ) => (
        event!(target: $target, $crate::Level::WARN, { $($field)* }, $($arg)*)
    );
    (target: $target:expr, $($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::WARN, { $($k).+ $($field)+ })
    );
    (target: $target:expr, ?$($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::WARN, { $($k).+ $($field)+ })
    );
    (target: $target:expr, %$($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::WARN, { $($k).+ $($field)+ })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::WARN, {}, $($arg)+)
    );
    ({ $($field:tt)+ }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,
            { $($field)+ },
            $($arg)+
        )
    );
    ($($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,
            { $($k).+ = $($field)*}
        )
    );
    (?$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,
            { ?$($k).+ = $($field)*}
        )
    );
    (%$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,
            { %$($k).+ = $($field)*}
        )
    );
    ($($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,
            { $($k).+, $($field)*}
        )
    );
    (?$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,
            { ?$($k).+, $($field)*}
        )
    );
    (%$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,
            { %$($k).+, $($field)*}
        )
    );
    ($($arg:tt)+) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,
            {},
            $($arg)+
        )
    );
}

/// Constructs an event at the error level.
///
/// When both a message and fields are included, curly braces (`{` and `}`) are
/// used to delimit the list of fields from the format string for the message.
/// A trailing comma on the final field is valid.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// use tokio_trace::field;
/// let (err_info, port) = ("No connection", 22);
///
/// error!(port = port, error = field::display(err_info));
/// error!(target: "app_events", "App Error: {}", err_info);
/// error!({ info = err_info }, "error on port: {}", port);
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! error {
    (target: $target:expr, { $($field:tt)* }, $($arg:tt)* ) => (
        event!(target: $target, $crate::Level::ERROR, { $($field)* }, $($arg)*)
    );
    (target: $target:expr, $($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::ERROR, { $($k).+ $($field)+ })
    );
    (target: $target:expr, ?$($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::ERROR, { $($k).+ $($field)+ })
    );
    (target: $target:expr, %$($k:ident).+ $($field:tt)+ ) => (
        event!(target: $target, $crate::Level::ERROR, { $($k).+ $($field)+ })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::ERROR, {}, $($arg)+)
    );
    ({ $($field:tt)+ }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            { $($field)+ },
            $($arg)+
        )
    );
    ($($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            { $($k).+ = $($field)*}
        )
    );
    (?$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            { ?$($k).+ = $($field)*}
        )
    );
    (%$($k:ident).+ = $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            { %$($k).+ = $($field)*}
        )
    );
    ($($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            { $($k).+, $($field)*}
        )
    );
    (?$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            { ?$($k).+, $($field)*}
        )
    );
    (%$($k:ident).+, $($field:tt)*) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            { %$($k).+, $($field)*}
        )
    );
    ($($arg:tt)+) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            {},
            $($arg)+
        )
    );
}

/// Constructs a new static callsite for a span or event.
#[doc(hidden)]
#[macro_export(local_inner_macros)]
macro_rules! callsite {
    (name: $name:expr, kind: $kind:expr, fields: $($fields:tt)*) => {{
        callsite! {
            name: $name,
            kind: $kind,
            target: __tokio_trace_module_path!(),
            level: $crate::Level::TRACE,
            fields: $($fields)*
        }
    }};
    (
        name: $name:expr,
        kind: $kind:expr,
        level: $lvl:expr,
        fields: $($fields:tt)*
    ) => {{
        callsite! {
            name: $name,
            kind: $kind,
            target: __tokio_trace_module_path!(),
            level: $lvl,
            fields: $($fields)*
        }
    }};
    (
        name: $name:expr,
        kind: $kind:expr,
        target: $target:expr,
        level: $lvl:expr,
        fields: $($fields:tt)*
    ) => {{
        use std::sync::{
            atomic::{self, AtomicUsize, Ordering},
            Once,
        };
        use $crate::{callsite, subscriber::Interest, Metadata};
        struct MyCallsite;
        static META: Metadata<'static> = {
            metadata! {
                name: $name,
                target: $target,
                level: $lvl,
                fields: fieldset!( $($fields)* ),
                callsite: &MyCallsite,
                kind: $kind,
            }
        };
        // FIXME: Rust 1.34 deprecated ATOMIC_USIZE_INIT. When Tokio's minimum
        // supported version is 1.34, replace this with the const fn `::new`.
        #[allow(deprecated)]
        static INTEREST: AtomicUsize = atomic::ATOMIC_USIZE_INIT;
        static REGISTRATION: Once = Once::new();
        impl MyCallsite {
            #[inline]
            fn interest(&self) -> Interest {
                match INTEREST.load(Ordering::Relaxed) {
                    0 => Interest::never(),
                    2 => Interest::always(),
                    _ => Interest::sometimes(),
                }
            }
        }
        impl callsite::Callsite for MyCallsite {
            fn set_interest(&self, interest: Interest) {
                let interest = match () {
                    _ if interest.is_never() => 0,
                    _ if interest.is_always() => 2,
                    _ => 1,
                };
                INTEREST.store(interest, Ordering::SeqCst);
            }

            fn metadata(&self) -> &Metadata {
                &META
            }
        }
        REGISTRATION.call_once(|| {
            callsite::register(&MyCallsite);
        });
        &MyCallsite
    }};
}

#[macro_export]
// TODO: determine if this ought to be public API?
#[doc(hidden)]
macro_rules! is_enabled {
    ($callsite:expr) => {{
        let interest = $callsite.interest();
        if interest.is_never() {
            false
        } else if interest.is_always() {
            true
        } else {
            let meta = $callsite.metadata();
            $crate::dispatcher::get_default(|current| current.enabled(meta))
        }
    }};
}

#[doc(hidden)]
#[macro_export(local_inner_macros)]
macro_rules! valueset {

    // === base case ===
    (@ { $($val:expr),* }, $next:expr, $(,)*) => {
        &[ $($val),* ]
    };

    // === recursive case (more tts), non-empty out set ===

    // TODO(#1138): determine a new syntax for uninitialized span fields, and
    // re-enable this.
    // (@{ $($out:expr),+ }, $next:expr, $($k:ident).+ = _, $($rest:tt)*) => {
    //     valueset!(@ { $($out),+, (&$next, None) }, $next, $($rest)*)
    // };
    (@ { $($out:expr),+ }, $next:expr, $($k:ident).+ = ?$val:expr, $($rest:tt)*) => {
        valueset!(
            @ { $($out),+, (&$next, Some(&debug(&$val) as &Value)) },
            $next,
            $($rest)*
        )
    };
    (@ { $($out:expr),+ }, $next:expr, $($k:ident).+ = %$val:expr, $($rest:tt)*) => {
        valueset!(
            @ { $($out),+, (&$next, Some(&display(&$val) as &Value)) },
            $next,
            $($rest)*
        )
    };
    (@ { $($out:expr),+ }, $next:expr, $($k:ident).+ = $val:expr, $($rest:tt)*) => {
        valueset!(
            @ { $($out),+, (&$next, Some(&$val as &Value)) },
            $next,
            $($rest)*
        )
    };
    (@ { $($out:expr),+ }, $next:expr, $($k:ident).+, $($rest:tt)*) => {
        valueset!(
            @ { $($out),+, (&$next, Some(&$($k).+ as &Value)) },
            $next,
            $($rest)*
        )
    };
    (@ { $($out:expr),+ }, $next:expr, ?$($k:ident).+, $($rest:tt)*) => {
        valueset!(
            @ { $($out),+, (&$next, Some(&debug(&$($k).+) as &Value)) },
            $next,
            $($rest)*
        )
    };
    (@ { $($out:expr),+ }, $next:expr, %$($k:ident).+, $($rest:tt)*) => {
        valueset!(
            @ { $($out),+, (&$next, Some(&display(&$($k).+) as &Value)) },
            $next,
            $($rest)*
        )
    };

    // == recursive case (more tts), empty out set ===

    // TODO(#1138): determine a new syntax for uninitialized span fields, and
    // re-enable this.
    // (@ { }, $next:expr, $($k:ident).+ = _, $($rest:tt)* ) => {
    //     valueset!(@ { (&$next, None) }, $next, $($rest)* )
    // };
    (@ { }, $next:expr, $($k:ident).+ = ?$val:expr, $($rest:tt)* ) => {
        valueset!(@ { (&$next, Some(&debug(&$val) as &Value)) }, $next, $($rest)* )
    };
    (@ { }, $next:expr, $($k:ident).+ = %$val:expr, $($rest:tt)*) => {
        valueset!(@ { (&$next, Some(&display(&$val) as &Value)) }, $next, $($rest)*)
    };
    (@ { }, $next:expr, $($k:ident).+ = $val:expr, $($rest:tt)*) => {
        valueset!(@ { (&$next, Some(&$val as &Value)) }, $next, $($rest)*)
    };
    (@ { }, $next:expr, $($k:ident).+, $($rest:tt)*) => {
        valueset!(@ { (&$next, Some(&$($k).+ as &Value)) }, $next, $($rest)* )
    };
    (@ { }, $next:expr, ?$($k:ident).+, $($rest:tt)*) => {
        valueset!(@ { (&$next, Some(&debug(&$($k).+) as &Value)) }, $next, $($rest)* )
    };
    (@ { }, $next:expr, %$($k:ident).+, $($rest:tt)*) => {
        valueset!(@ { (&$next, Some(&display(&$($k).+) as &Value)) }, $next, $($rest)* )
    };

    // === entry ===
    ($fields:expr, $($kvs:tt)+) => {
        {
            #[allow(unused_imports)]
            use $crate::field::{debug, display, Value};
            let mut iter = $fields.iter();
            $fields.value_set(valueset!(
                @ { },
                iter.next().expect("FieldSet corrupted (this is a bug)"),
                $($kvs)+,
            ))
        }
    };
    ($fields:expr,) => {
        {
            $fields.value_set(&[])
        }
    };
}

#[doc(hidden)]
#[macro_export(local_inner_macros)]
macro_rules! fieldset {
    // == base case ==
    (@ { $($out:expr),* $(,)* } $(,)*) => {
        &[ $($out),* ]
    };

    // == empty out set, remaining tts ==
    (@ { } $($k:ident).+ = ?$val:expr, $($rest:tt)*) => {
        fieldset!(@ { __tokio_trace_stringify!($($k).+) } $($rest)*)
    };
    (@ { } $($k:ident).+ = %$val:expr, $($rest:tt)*) => {
        fieldset!(@ { __tokio_trace_stringify!($($k).+) } $($rest)*)
    };
    (@ { } $($k:ident).+ = $val:expr, $($rest:tt)*) => {
        fieldset!(@ { __tokio_trace_stringify!($($k).+) } $($rest)*)
    };
    // TODO(#1138): determine a new syntax for uninitialized span fields, and
    // re-enable this.
    // (@ { } $($k:ident).+ = _, $($rest:tt)*) => {
    //     fieldset!(@ { __tokio_trace_stringify!($($k).+) } $($rest)*)
    // };
    (@ { } ?$($k:ident).+, $($rest:tt)*) => {
        fieldset!(@ { __tokio_trace_stringify!($($k).+) } $($rest)*)
    };
    (@ { } %$($k:ident).+, $($rest:tt)*) => {
        fieldset!(@ { __tokio_trace_stringify!($($k).+) } $($rest)*)
    };
    (@ { } $($k:ident).+, $($rest:tt)*) => {
        fieldset!(@ { __tokio_trace_stringify!($($k).+) } $($rest)*)
    };


    // == non-empty out set, remaining tts ==
    (@ { $($out:expr),+ } $($k:ident).+ = ?$val:expr, $($rest:tt)*) => {
        fieldset!(@ { $($out),+,__tokio_trace_stringify!($($k).+) } $($rest)*)
    };
    (@ { $($out:expr),+ } $($k:ident).+ = %$val:expr, $($rest:tt)*) => {
        fieldset!(@ { $($out),+, __tokio_trace_stringify!($($k).+) } $($rest)*)
    };
    (@ { $($out:expr),+ } $($k:ident).+ = $val:expr, $($rest:tt)*) => {
        fieldset!(@ { $($out),+, __tokio_trace_stringify!($($k).+) } $($rest)*)
    };
    // TODO(#1138): determine a new syntax for uninitialized span fields, and
    // re-enable this.
    // (@ { $($out:expr),+ } $($k:ident).+ = _, $($rest:tt)*) => {
    //     fieldset!(@ { $($out),+, __tokio_trace_stringify!($($k).+) } $($rest)*)
    // };
    (@ { $($out:expr),+ } ?$($k:ident).+, $($rest:tt)*) => {
        fieldset!(@ { $($out),+, __tokio_trace_stringify!($($k).+) } $($rest)*)
    };
    (@ { $($out:expr),+ } %$($k:ident).+, $($rest:tt)*) => {
        fieldset!(@ { $($out),+, __tokio_trace_stringify!($($k).+) } $($rest)*)
    };
    (@ { $($out:expr),+ } $($k:ident).+, $($rest:tt)*) => {
        fieldset!(@ { $($out),+, __tokio_trace_stringify!($($k).+) } $($rest)*)
    };

    // == entry ==
    ($($args:tt)*) => {
        fieldset!(@ { } $($args)*, )
    };

}

// The macros above cannot invoke format_args directly because they use
// local_inner_macros. A format_args invocation there would resolve to
// $crate::format_args, which does not exist. Instead invoke format_args here
// outside of local_inner_macros so that it resolves (probably) to
// core::format_args or std::format_args. Same for the several macros that
// follow.
//
// This is a workaround until we drop support for pre-1.30 compilers. At that
// point we can remove use of local_inner_macros, use $crate:: when invoking
// local macros, and invoke format_args directly.
#[doc(hidden)]
#[macro_export]
macro_rules! __tokio_trace_format_args {
    ($($args:tt)*) => {
        format_args!($($args)*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __tokio_trace_module_path {
    () => {
        module_path!()
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __tokio_trace_file {
    () => {
        file!()
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __tokio_trace_line {
    () => {
        line!()
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __tokio_trace_concat {
    ($($e:expr),*) => {
        concat!($($e),*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __tokio_trace_stringify {
    ($s:expr) => {
        stringify!($s)
    };
}

#[cfg(feature = "log")]
#[doc(hidden)]
#[macro_export]
macro_rules! level_to_log {
    ($level:expr) => {
        match $level {
            $crate::Level::ERROR => $crate::log::Level::Error,
            $crate::Level::WARN => $crate::log::Level::Warn,
            $crate::Level::INFO => $crate::log::Level::Info,
            $crate::Level::INFO => $crate::log::Level::Debug,
            _ => $crate::log::Level::Trace,
        }
    };
}

#[cfg(feature = "log")]
#[doc(hidden)]
#[macro_export(local_inner_macros)]
macro_rules! __tokio_trace_log {
    (target: $target:expr, $level:expr, $($field:tt)+ ) => {
        use $crate::log;
        let level = level_to_log!($level);
        if level <= log::STATIC_MAX_LEVEL {
            let log_meta = log::Metadata::builder()
                .level(level)
                .target($target)
                .build();
            let logger = log::logger();
            if logger.enabled(&log_meta) {
                logger.log(&log::Record::builder()
                    .file(Some(__tokio_trace_file!()))
                    .module_path(Some(__tokio_trace_module_path!()))
                    .line(Some(__tokio_trace_line!()))
                    .metadata(log_meta)
                    .args(__mk_format_args!($($field)+))
                    .build());
            }
        }
    };
}

#[cfg(not(feature = "log"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __tokio_trace_log {
    (target: $target:expr, $level:expr, $($field:tt)+ ) => {};
}

#[cfg(feature = "log")]
#[doc(hidden)]
#[macro_export]
macro_rules! __tokio_trace_disabled_span {
    ($meta:expr, $valueset:expr) => {{
        let span = $crate::Span::new_disabled($meta);
        span.record_all(&$valueset);
        span
    }};
}

#[cfg(not(feature = "log"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __tokio_trace_disabled_span {
    ($meta:expr, $valueset:expr) => {
        $crate::Span::new_disabled($meta)
    };
}

#[cfg(feature = "log")]
#[doc(hidden)]
#[macro_export(local_inner_macros)]
macro_rules! __mk_format_string {
    // === base case ===
    (@ { $($out:expr),+ } $(,)*) => {
        __tokio_trace_concat!( $($out),+)
    };

    // === recursive case (more tts), non-empty out set ===
    (@ { $($out:expr),+ }, message = $val:expr, $($rest:tt)*) => {
        __mk_format_string!(@ { $($out),+, "{} " }, $($rest)*)
    };
    (@ { $($out:expr),+ }, $($k:ident).+ = ?$val:expr, $($rest:tt)*) => {
        __mk_format_string!(@ { $($out),+, __tokio_trace_stringify!($($k).+), "={:?} " }, $($rest)*)
    };
    (@ { $($out:expr),+ }, $($k:ident).+ = %$val:expr, $($rest:tt)*) => {
        __mk_format_string!(@ { $($out),+, __tokio_trace_stringify!($($k).+), "={} " }, $($rest)*)
    };
    (@ { $($out:expr),+ }, $($k:ident).+ = $val:expr, $($rest:tt)*) => {
        __mk_format_string!(@ { $($out),+, __tokio_trace_stringify!($($k).+), "={:?} " }, $($rest)*)
    };

    // === recursive case (more tts), empty out set ===
    (@ { }, message = $val:expr, $($rest:tt)*) => {
        __mk_format_string!(@ { "{} " }, $($rest)*)
    };
    (@ { }, $($k:ident).+ = ?$val:expr, $($rest:tt)*) => {
        __mk_format_string!(@ { __tokio_trace_stringify!($($k).+), "={:?} " }, $($rest)*)
    };
    (@ { }, $($k:ident).+ = %$val:expr, $($rest:tt)*) => {
        __mk_format_string!(@ {  __tokio_trace_stringify!($($k).+), "={} " }, $($rest)*)
    };
    (@ { }, $($k:ident).+ = $val:expr, $($rest:tt)*) => {
        __mk_format_string!(@ { __tokio_trace_stringify!($($k).+), "={:?} " }, $($rest)*)
    };

    // === entry ===
    ($($kvs:tt)+) => {
        __mk_format_string!(@ { }, $($kvs)+,)
    };
    () => {
        ""
    }
}

#[cfg(feature = "log")]
#[doc(hidden)]
#[macro_export(local_inner_macros)]
macro_rules! __mk_format_args {
    // == base case ==
    (@ { $($out:expr),* }, $fmt:expr, $(,)*) => {
        __tokio_trace_format_args!($fmt, $($out),*)
    };

    // === recursive case (more tts), non-empty out set ===
    (@ { $($out:expr),+ }, $fmt:expr, $($k:ident).+ = ?$val:expr, $($rest:tt)*) => {
        __mk_format_args!(@ { $($out),+, $val }, $fmt, $($rest)*)
    };
    (@ { $($out:expr),+ }, $fmt:expr, $($k:ident).+ = %$val:expr, $($rest:tt)*) => {
        __mk_format_args!(@ { $($out),+, $val }, $fmt, $($rest)*)
    };
    (@ { $($out:expr),+ }, $fmt:expr, $($k:ident).+ = $val:expr, $($rest:tt)*) => {
        __mk_format_args!(@ { $($out),+, $val }, $fmt, $($rest)*)
    };

    // == recursive case (more tts), empty out set ===
    (@ { }, $fmt:expr, message = $val:expr, $($rest:tt)*) => {
        __mk_format_args!(@ { $val }, $fmt, $($rest)*)
    };
    (@ { }, $fmt:expr, $($k:ident).+ = ?$val:expr, $($rest:tt)*) => {
        __mk_format_args!(@ { $val }, $fmt, $($rest)*)
    };
    (@ { }, $fmt:expr, $($k:ident).+ = %$val:expr, $($rest:tt)*) => {
        __mk_format_args!(@ { $val }, $fmt, $($rest)*)
    };
    (@ { }, $fmt:expr, $($k:ident).+ = $val:expr, $($rest:tt)*) => {
        __mk_format_args!(@ { $val }, $fmt, $($rest)*)
    };

    // === entry ===
    ($($kv:tt)*) => {
        __mk_format_args!(@ { }, __mk_format_string!($($kv)*), $($kv)*,)
    };
}
