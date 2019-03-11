/// Constructs a new span.
///
/// # Examples
///
/// Creating a new span with no fields:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// let mut span = span!("my span");
/// span.enter(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
///
/// Creating a span with fields:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// span!("my span", foo = 2, bar = "a string").enter(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
///
/// Note that a trailing comma on the final field is valid:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// span!(
///     "my span",
///     foo = 2,
///     bar = "a string",
/// );
/// # }
/// ```
///
/// Creating a span with custom target and log level:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// span!(
///     target: "app_span",
///     level: tokio_trace::Level::TRACE,
///     "my span",
///     foo = 3,
///     bar = "another string"
/// );
/// # }
/// ```
///
/// Field values may be recorded after the span is created:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// let mut my_span = span!("my span", foo = 2, bar);
/// my_span.record("bar", &7);
/// # }
/// ```
///
/// Note that a span may have up to 32 fields. The following will not compile:
/// ```rust,compile_fail
///  # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// span!(
///     "too many fields!",
///     a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9,
///     j = 10, k = 11, l = 12, m = 13, n = 14, o = 15, p = 16, q = 17,
///     r = 18, s = 19, t = 20, u = 21, v = 22, w = 23, x = 24, y = 25,
///     z = 26, aa = 27, bb = 28, cc = 29, dd = 30, ee = 31, ff = 32, gg = 33
/// );
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! span {
    (
        target: $target:expr,
        level: $lvl:expr,
        parent: $parent:expr,
        $name:expr,
        $($k:ident $( = $val:expr )* ),*,
    ) => {
        span!(
            target: $target,
            level: $lvl,
            parent: $parent,
            $name,
            $($k $( = $val)*),*
        )
    };
    (
        target: $target:expr,
        level: $lvl:expr,
        parent: $parent:expr,
        $name:expr,
        $($k:ident $( = $val:expr )* ),*
    ) => {
        {
            use $crate::callsite;
            use $crate::callsite::Callsite;
            let callsite = callsite! {
                name: $name,
                target: $target,
                level: $lvl,
                fields: $($k),*
            };
            if is_enabled!(callsite) {
                let meta = callsite.metadata();
                $crate::Span::child_of(
                    $parent,
                    meta,
                    &valueset!(meta.fields(), $($k $( = $val)*),*),
                )
            } else {
                $crate::Span::new_disabled()
            }
        }
    };
    (
        target: $target:expr,
        level: $lvl:expr,
        $name:expr,
        $($k:ident $( = $val:expr )* ),*
    ) => {
        {
            use $crate::callsite;
            use $crate::callsite::Callsite;
            let callsite = callsite! {
                name: $name,
                target: $target,
                level: $lvl,
                fields: $($k),*
            };
            if is_enabled!(callsite) {
                let meta = callsite.metadata();
                $crate::Span::new(
                    meta,
                    &valueset!(meta.fields(), $($k $( = $val)*),*),
                )
            } else {
                $crate::Span::new_disabled()
            }
        }
    };
    (target: $target:expr, level: $lvl:expr, parent: $parent:expr, $name:expr) => {
        span!(target: $target, level: $lvl, parent: $parent, $name,)
    };
    (level: $lvl:expr, parent: $parent:expr, $name:expr, $($k:ident $( = $val:expr )* ),*,) => {
        span!(
            target: __tokio_trace_module_path!(),
            level: $lvl,
            parent: $parent,
            $name,
            $($k $( = $val)*),*
        )
    };
    (level: $lvl:expr, parent: $parent:expr, $name:expr, $($k:ident $( = $val:expr )* ),*) => {
        span!(
            target: __tokio_trace_module_path!(),
            level: $lvl,
            parent: $parent,
            $name,
            $($k $( = $val)*),*
        )
    };
    (level: $lvl:expr, parent: $parent:expr, $name:expr) => {
        span!(target: __tokio_trace_module_path!(), level: $lvl, parent: $parent, $name,)
    };
    (parent: $parent:expr, $name:expr, $($k:ident $( = $val:expr)*),*,) => {
        span!(
            target: __tokio_trace_module_path!(),
            level: $crate::Level::TRACE,
            parent: $parent,
            $name,
            $($k $( = $val)*),*
        )
    };
    (parent: $parent:expr, $name:expr, $($k:ident $( = $val:expr)*),*) => {
        span!(
            target: __tokio_trace_module_path!(),
            level: $crate::Level::TRACE,
            parent: $parent,
            $name,
            $($k $( = $val)*),*
        )
    };
    (parent: $parent:expr, $name:expr) => {
        span!(
            target: __tokio_trace_module_path!(),
            level: $crate::Level::TRACE,
            parent: $parent,
            $name,
        )
    };
    (
        target: $target:expr,
        level: $lvl:expr,
        $name:expr,
        $($k:ident $( = $val:expr )* ),*,
    ) => {
        span!(
            target: $target,
            level: $lvl,
            $name,
            $($k $( = $val)*),*
        )
    };
    (
        target: $target:expr,
        level: $lvl:expr,
        $name:expr,
        $($k:ident $( = $val:expr )* ),*
    ) => {
        span!(
            target: $target,
            level: $lvl,
            $name,
            $($k $( = $val)*),*
        )
    };
    (target: $target:expr, level: $lvl:expr, $name:expr) => {
        span!(target: $target, level: $lvl, $name,)
    };
    (target: $target:expr, level: $lvl:expr, $name:expr,) => {
        span!(
            target: $target,
            level: $lvl,
            $name,
        )
    };
    (level: $lvl:expr, $name:expr, $($k:ident $( = $val:expr )* ),*,) => {
        span!(
            target: __tokio_trace_module_path!(),
            level: $lvl,
            $name,
            $($k $( = $val)*),*
        )
    };
    (level: $lvl:expr, $name:expr, $($k:ident $( = $val:expr )* ),*) => {
        span!(
            target: __tokio_trace_module_path!(),
            level: $lvl,
            $name, $($k $( = $val)*),*
        )
    };
    (level: $lvl:expr, $name:expr) => {
        span!(target: __tokio_trace_module_path!(), level: $lvl, $name,)
    };
    ($name:expr, $($k:ident $( = $val:expr)*),*,) => {
        span!(
            target: __tokio_trace_module_path!(),
            level: $crate::Level::TRACE,
            $name,
            $($k $( = $val)*),*
        )
    };
    ($name:expr, $($k:ident $( = $val:expr)*),*) => {
        span!(
            target: __tokio_trace_module_path!(),
            level: $crate::Level::TRACE,
            $name,
            $($k $( = $val)*),*
        )
    };
    ($name:expr) => {
        span!(
            target: __tokio_trace_module_path!(),
            level: $crate::Level::TRACE,
            $name,
        )
    };
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
/// use tokio_trace::{Level, field};
///
/// # fn main() {
///     event!(Level::Info, foo = 5, bad_field, bar = field::display("hello"))
/// #}
/// ```
///
/// Events may have up to 32 fields. The following will not compile:
/// ```rust,compile_fail
///  # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// event!(tokio_trace::Level::INFO,
///     a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9,
///     j = 10, k = 11, l = 12, m = 13, n = 14, o = 15, p = 16, q = 17,
///     r = 18, s = 19, t = 20, u = 21, v = 22, w = 23, x = 24, y = 25,
///     z = 26, aa = 27, bb = 28, cc = 29, dd = 30, ee = 31, ff = 32, gg = 33
/// );
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! event {
    (target: $target:expr, $lvl:expr, { $( $k:ident = $val:expr ),* $(,)*} )=> ({
        {
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
                target: $target,
                level: $lvl,
                fields: $( $k ),*
            };
            if is_enabled!(callsite) {
                let meta = callsite.metadata();
                Event::dispatch(meta, &valueset!(meta.fields(), $( $k = $val),* ));
            }
        }
    });
    (
        target: $target:expr,
        $lvl:expr,
        { $( $k:ident = $val:expr ),*, },
        $($arg:tt)+
    ) => ({
        event!(
            target: $target,
            $lvl,
            { message = __tokio_trace_format_args!($($arg)+), $( $k = $val ),* }
        )
    });
    (
        target: $target:expr,
        $lvl:expr,
        { $( $k:ident = $val:expr ),* },
        $($arg:tt)+
    ) => ({
        event!(
            target: $target,
            $lvl,
            { message = __tokio_trace_format_args!($($arg)+), $( $k = $val ),* }
        )
    });
    (target: $target:expr, $lvl:expr, $( $k:ident = $val:expr ),+, ) => (
        event!(target: $target, $lvl, { $($k = $val),+ })
    );
    (target: $target:expr, $lvl:expr, $( $k:ident = $val:expr ),+ ) => (
        event!(target: $target, $lvl, { $($k = $val),+ })
    );
    (target: $target:expr, $lvl:expr, $($arg:tt)+ ) => (
        event!(target: $target, $lvl, { }, $($arg)+)
    );
    ( $lvl:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $lvl,
            { message = __tokio_trace_format_args!($($arg)+), $($k = $val),* }
        )
    );
    ( $lvl:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $lvl,
            { message = __tokio_trace_format_args!($($arg)+), $($k = $val),* }
        )
    );
    ( $lvl:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: __tokio_trace_module_path!(), $lvl, { $($k = $val),* })
    );
    ( $lvl:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: __tokio_trace_module_path!(), $lvl, { $($k = $val),* })
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
    (target: $target:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::TRACE, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::TRACE, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: $target, $crate::Level::TRACE, { $($k = $val),* })
    );
    (target: $target:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: $target, $crate::Level::TRACE, { $($k = $val),* })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        drop(event!(target: $target, $crate::Level::TRACE, {}, $($arg)+));
    );
    ({ $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            { $($k = $val),* },
            $($arg)+
        )
    );
    ({ $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            { $($k = $val),* },
            $($arg)+
        )
    );
    ($( $k:ident = $val:expr ),*, ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            { $($k = $val),* }
        )
    );
    ($( $k:ident = $val:expr ),* ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::TRACE,
            { $($k = $val),* }
        )
    );
    ($($arg:tt)+ ) => (
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
/// debug!(x = field::debug(pos.x), y = field::debug(pos.y));
/// debug!(target: "app_events", { position = field::debug(pos) }, "New position");
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! debug {
    (target: $target:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::DEBUG, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::DEBUG, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: $target, $crate::Level::DEBUG, { $($k = $val),* })
    );
    (target: $target:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: $target, $crate::Level::DEBUG, { $($k = $val),* })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::DEBUG, {}, $($arg)+)
    );
    ({ $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(), $crate::Level::DEBUG, { $($k = $val),* }, $($arg)+)
    );
    ({ $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::DEBUG,
            { $($k = $val),* },
            $($arg)+
        )
    );
    ($( $k:ident = $val:expr ),*, ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::DEBUG,
            { $($k = $val),* }
        )
    );
    ($( $k:ident = $val:expr ),* ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::DEBUG,
            { $($k = $val),* }
        )
    );
    ($($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::DEBUG,
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
/// let conn_info = Connection { port: 40, speed: 3.20 };
///
/// info!({ port = conn_info.port }, "connected to {}", addr);
/// info!(
///     target: "connection_events",
///     ip = field::display(addr),
///     port = conn_info.port,
///     speed = field::debug(conn_info.speed)
/// );
/// # }
/// ```
#[macro_export(local_inner_macros)]
macro_rules! info {
    (target: $target:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: $target, $crate::Level::INFO, { $($k = $val),* })
    );
    (target: $target:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: $target, $crate::Level::INFO, { $($k = $val),* })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, {}, $($arg)+)
    );
    ({ $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { $($k = $val),* },
            $($arg)+
        )
    );
    ({ $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { $($k = $val),* },
            $($arg)+
        )
    );
    ($( $k:ident = $val:expr ),*, ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { $($k = $val),* }
        )
    );
    ($( $k:ident = $val:expr ),* ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::INFO,
            { $($k = $val),* }
        )
    );
    ($($arg:tt)+ ) => (
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
    (target: $target:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::WARN, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::WARN, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: $target, $crate::Level::WARN, { $($k = $val),* })
    );
    (target: $target:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: $target, $crate::Level::WARN, { $($k = $val),* })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        drop(event!(target: $target, $crate::Level::WARN, {}, $($arg)+));
    );
    ({ $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,
            { $($k = $val),* },
            $($arg)+
        )
    );
    ({ $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,
            { $($k = $val),* },
            $($arg)+
        )
    );
    ($( $k:ident = $val:expr ),*, ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,{ $($k = $val),* }
        )
    );
    ($( $k:ident = $val:expr ),* ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::WARN,
            { $($k = $val),* }
        )
    );
    ($($arg:tt)+ ) => (
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
    (target: $target:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::ERROR, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::ERROR, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: $target, $crate::Level::ERROR, { $($k = $val),* })
    );
    (target: $target:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: $target, $crate::Level::ERROR, { $($k = $val),* })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::ERROR, {}, $($arg)+)
    );
    ({ $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            { $($k = $val),* },
            $($arg)+
        )
    );
    ({ $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            { $($k = $val),* },
            $($arg)+
        )
    );
    ($( $k:ident = $val:expr ),*, ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            { $($k = $val),* }
        )
    );
    ($( $k:ident = $val:expr ),* ) => (
        event!(
            target: __tokio_trace_module_path!(),
            $crate::Level::ERROR,
            { $($k = $val),* }
        )
    );
    ($($arg:tt)+ ) => (
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
    (name: $name:expr, fields: $( $field_name:expr ),* $(,)*) => ({
        callsite! {
            name: $name,
            target: __tokio_trace_module_path!(),
            level: $crate::Level::TRACE,
            fields: $( $field_name ),*
        }
    });
    (name: $name:expr, level: $lvl:expr, fields: $( $field_name:expr ),* $(,)*) => ({
        callsite! {
            name: $name,
            target: __tokio_trace_module_path!(),
            level: $lvl,
            fields: $( $field_name ),*
        }
    });
    (
        name: $name:expr,
        target: $target:expr,
        level: $lvl:expr,
        fields: $( $field_name:expr ),*
        $(,)*
    ) => ({
        use std::sync::{Once, atomic::{self, AtomicUsize, Ordering}};
        use $crate::{callsite, Metadata, subscriber::Interest};
        struct MyCallsite;
        static META: Metadata<'static> = {
            metadata! {
                name: $name,
                target: $target,
                level: $lvl,
                fields: &[ $( __tokio_trace_stringify!($field_name) ),* ],
                callsite: &MyCallsite,
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
            fn add_interest(&self, interest: Interest) {
                let current_interest = self.interest();
                let interest = match () {
                    // If the added interest is `never()`, don't change anything
                    // — either a different subscriber added a higher
                    // interest, which we want to preserve, or the interest is 0
                    // anyway (as it's initialized to 0).
                    _ if interest.is_never() => return,
                    // If the interest is `sometimes()`, that overwrites a `never()`
                    // interest, but doesn't downgrade an `always()` interest.
                    _ if interest.is_sometimes() && current_interest.is_never() => 1,
                    // If the interest is `always()`, we overwrite the current
                    // interest, as always() is the highest interest level and
                    // should take precedent.
                    _ if interest.is_always() => 2,
                    _ => return,
                };
                INTEREST.store(interest, Ordering::Relaxed);
            }
            fn clear_interest(&self) {
                INTEREST.store(0, Ordering::Relaxed);
            }
            fn metadata(&self) -> &Metadata {
                &META
            }
        }
        REGISTRATION.call_once(|| {
            callsite::register(&MyCallsite);
        });
        &MyCallsite
    })
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
    ($fields:expr, $($k:ident $( = $val:expr )* ) ,*) => {
        {
            let mut iter = $fields.iter();
            $fields.value_set(&[
                $((
                    &iter.next().expect("FieldSet corrupted (this is a bug)"),
                    valueset!(@val $k $(= $val)*)
                )),*
            ])
        }
    };
    (@val $k:ident = $val:expr) => {
        Some(&$val as &$crate::field::Value)
    };
    (@val $k:ident) => { None };
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
