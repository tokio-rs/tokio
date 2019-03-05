//! A simple example demonstrating how one might implement a custom
//! subscriber.
//!
//! This subscriber implements a tree-structured logger similar to
//! the "compact" formatter in [`slog-term`]. The demo mimicks the
//! example output in the screenshot in the [`slog` README].
//!
//! Note that this logger isn't ready for actual production use.
//! Several corners were cut to make the example simple.
//!
//! [`slog-term`]: https://docs.rs/slog-term/2.4.0/slog_term/
//! [`slog` README]: https://github.com/slog-rs/slog#terminal-output-example
extern crate ansi_term;
extern crate humantime;
use self::ansi_term::{Color, Style};
use super::tokio_trace::{
    self,
    field::{Field, Record},
    Id, Level, Subscriber,
};

use std::{
    cell::RefCell,
    collections::HashMap,
    fmt,
    io::{self, Write},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    thread,
    time::SystemTime,
};

/// Tracks the currently executing span on a per-thread basis.
#[derive(Clone)]
pub struct CurrentSpanPerThread {
    current: &'static thread::LocalKey<RefCell<Vec<Id>>>,
}

impl CurrentSpanPerThread {
    pub fn new() -> Self {
        thread_local! {
            static CURRENT: RefCell<Vec<Id>> = RefCell::new(vec![]);
        };
        Self { current: &CURRENT }
    }

    /// Returns the [`Id`](::Id) of the span in which the current thread is
    /// executing, or `None` if it is not inside of a span.
    pub fn id(&self) -> Option<Id> {
        self.current
            .with(|current| current.borrow().last().cloned())
    }

    pub fn enter(&self, span: Id) {
        self.current.with(|current| {
            current.borrow_mut().push(span);
        })
    }

    pub fn exit(&self) {
        self.current.with(|current| {
            let _ = current.borrow_mut().pop();
        })
    }
}

pub struct SloggishSubscriber {
    // TODO: this can probably be unified with the "stack" that's used for
    // printing?
    current: CurrentSpanPerThread,
    indent_amount: usize,
    stderr: io::Stderr,
    stack: Mutex<Vec<Id>>,
    spans: Mutex<HashMap<Id, Span>>,
    ids: AtomicUsize,
}

struct Span {
    parent: Option<Id>,
    kvs: Vec<(&'static str, String)>,
}

struct Event<'a> {
    stderr: io::StderrLock<'a>,
    comma: bool,
}

struct ColorLevel<'a>(&'a Level);

impl<'a> fmt::Display for ColorLevel<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            &Level::TRACE => Color::Purple.paint("TRACE"),
            &Level::DEBUG => Color::Blue.paint("DEBUG"),
            &Level::INFO => Color::Green.paint("INFO "),
            &Level::WARN => Color::Yellow.paint("WARN "),
            &Level::ERROR => Color::Red.paint("ERROR"),
        }
        .fmt(f)
    }
}

impl Span {
    fn new(
        parent: Option<Id>,
        _meta: &tokio_trace::Metadata,
        values: &tokio_trace::field::ValueSet,
    ) -> Self {
        let mut span = Self {
            parent,
            kvs: Vec::new(),
        };
        values.record(&mut span);
        span
    }
}

impl Record for Span {
    fn record_debug(&mut self, field: &Field, value: &fmt::Debug) {
        self.kvs.push((field.name(), format!("{:?}", value)))
    }
}

impl<'a> Record for Event<'a> {
    fn record_debug(&mut self, field: &Field, value: &fmt::Debug) {
        write!(
            &mut self.stderr,
            "{comma} ",
            comma = if self.comma { "," } else { "" },
        )
        .unwrap();
        let name = field.name();
        if name == "message" {
            write!(
                &mut self.stderr,
                "{}",
                // Have to alloc here due to `ansi_term`'s API...
                Style::new().bold().paint(format!("{:?}", value))
            )
            .unwrap();
            self.comma = true;
        } else {
            write!(
                &mut self.stderr,
                "{}: {:?}",
                Style::new().bold().paint(name),
                value
            )
            .unwrap();
            self.comma = true;
        }
    }
}

impl SloggishSubscriber {
    pub fn new(indent_amount: usize) -> Self {
        Self {
            current: CurrentSpanPerThread::new(),
            indent_amount,
            stderr: io::stderr(),
            stack: Mutex::new(vec![]),
            spans: Mutex::new(HashMap::new()),
            ids: AtomicUsize::new(0),
        }
    }

    fn print_kvs<'a, I, K, V>(
        &self,
        writer: &mut impl Write,
        kvs: I,
        leading: &str,
    ) -> io::Result<()>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str> + 'a,
        V: fmt::Display + 'a,
    {
        let mut kvs = kvs.into_iter();
        if let Some((k, v)) = kvs.next() {
            write!(
                writer,
                "{}{}: {}",
                leading,
                Style::new().bold().paint(k.as_ref()),
                v
            )?;
        }
        for (k, v) in kvs {
            write!(writer, ", {}: {}", Style::new().bold().paint(k.as_ref()), v)?;
        }
        Ok(())
    }

    fn print_indent(&self, writer: &mut impl Write, indent: usize) -> io::Result<()> {
        for _ in 0..(indent * self.indent_amount) {
            write!(writer, " ")?;
        }
        Ok(())
    }
}

impl Subscriber for SloggishSubscriber {
    fn enabled(&self, _metadata: &tokio_trace::Metadata) -> bool {
        true
    }

    fn new_span(&self, span: &tokio_trace::span::Attributes) -> tokio_trace::Id {
        let meta = span.metadata();
        let values = span.values();
        let next = self.ids.fetch_add(1, Ordering::SeqCst) as u64;
        let id = tokio_trace::Id::from_u64(next);
        let span = Span::new(self.current.id(), meta, values);
        self.spans.lock().unwrap().insert(id.clone(), span);
        id
    }

    fn record(&self, span: &tokio_trace::Id, values: &tokio_trace::field::ValueSet) {
        let mut spans = self.spans.lock().expect("mutex poisoned!");
        if let Some(span) = spans.get_mut(span) {
            values.record(span);
        }
    }

    fn record_follows_from(&self, _span: &tokio_trace::Id, _follows: &tokio_trace::Id) {
        // unimplemented
    }

    fn enter(&self, span_id: &tokio_trace::Id) {
        self.current.enter(span_id.clone());
        let mut stderr = self.stderr.lock();
        let mut stack = self.stack.lock().unwrap();
        let spans = self.spans.lock().unwrap();
        let data = spans.get(span_id);
        let parent = data.and_then(|span| span.parent.as_ref());
        if stack.iter().any(|id| id == span_id) {
            // We are already in this span, do nothing.
            return;
        } else {
            let indent = if let Some(idx) = stack
                .iter()
                .position(|id| parent.map(|p| id == p).unwrap_or(false))
            {
                let idx = idx + 1;
                stack.truncate(idx);
                idx
            } else {
                stack.clear();
                0
            };
            self.print_indent(&mut stderr, indent).unwrap();
            stack.push(span_id.clone());
            if let Some(data) = data {
                self.print_kvs(&mut stderr, data.kvs.iter().map(|(k, v)| (k, v)), "")
                    .unwrap();
            }
            write!(&mut stderr, "\n").unwrap();
        }
    }

    fn event(&self, event: &tokio_trace::Event) {
        let mut stderr = self.stderr.lock();
        let indent = self.stack.lock().unwrap().len();
        self.print_indent(&mut stderr, indent).unwrap();
        write!(
            &mut stderr,
            "{timestamp} {level} {target}",
            timestamp = humantime::format_rfc3339_seconds(SystemTime::now()),
            level = ColorLevel(event.metadata().level()),
            target = &event.metadata().target(),
        )
        .unwrap();
        let mut recorder = Event {
            stderr,
            comma: false,
        };
        event.record(&mut recorder);
        write!(&mut recorder.stderr, "\n").unwrap();
    }

    #[inline]
    fn exit(&self, _span: &tokio_trace::Id) {
        // TODO: unify stack with current span
        self.current.exit();
    }

    fn drop_span(&self, _id: tokio_trace::Id) {
        // TODO: GC unneeded spans.
    }
}
