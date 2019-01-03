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
    subscriber::{self, Subscriber},
    Id, Level,
};

use std::{
    collections::HashMap,
    fmt,
    io::{self, Write},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::SystemTime,
};

pub struct SloggishSubscriber {
    // TODO: this can probably be unified with the "stack" that's used for
    // printing?
    current: subscriber::CurrentSpanPerThread,
    indent_amount: usize,
    stderr: io::Stderr,
    stack: Mutex<Vec<Id>>,
    spans: Mutex<HashMap<Id, Span>>,
    events: Mutex<HashMap<Id, Event>>,
    ids: AtomicUsize,
}

struct Span {
    parent: Option<Id>,
    kvs: Vec<(String, String)>,
}

struct Event {
    level: tokio_trace::Level,
    target: String,
    message: String,
    kvs: Vec<(String, String)>,
}

struct ColorLevel(Level);

impl fmt::Display for ColorLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            Level::TRACE => Color::Purple.paint("TRACE"),
            Level::DEBUG => Color::Blue.paint("DEBUG"),
            Level::INFO => Color::Green.paint("INFO"),
            Level::WARN => Color::Yellow.paint("WARN "),
            Level::ERROR => Color::Red.paint("ERROR"),
        }
        .fmt(f)
    }
}

impl Span {
    fn new(parent: Option<Id>, _meta: &tokio_trace::Metadata) -> Self {
        Self {
            parent,
            kvs: Vec::new(),
        }
    }

    fn record(&mut self, key: &tokio_trace::field::Field, value: fmt::Arguments) {
        // TODO: shouldn't have to alloc the key...
        let k = key.name().unwrap_or("???").to_owned();
        let v = fmt::format(value);
        self.kvs.push((k, v));
    }
}

impl Event {
    fn new(meta: &tokio_trace::Metadata) -> Self {
        Self {
            target: meta.target.to_owned(),
            level: meta.level.clone(),
            message: String::new(),
            kvs: Vec::new(),
        }
    }

    fn record(&mut self, key: &tokio_trace::field::Field, value: fmt::Arguments) {
        if key.name() == Some("message") {
            self.message = fmt::format(value);
            return;
        }

        // TODO: shouldn't have to alloc the key...
        let k = key.name().unwrap_or("???").to_owned();
        let v = fmt::format(value);
        self.kvs.push((k, v));
    }
}

impl SloggishSubscriber {
    pub fn new(indent_amount: usize) -> Self {
        Self {
            current: subscriber::CurrentSpanPerThread::new(),
            indent_amount,
            stderr: io::stderr(),
            stack: Mutex::new(vec![]),
            spans: Mutex::new(HashMap::new()),
            events: Mutex::new(HashMap::new()),
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

    fn new_span(&self, span: &tokio_trace::Metadata) -> tokio_trace::Id {
        let next = self.ids.fetch_add(1, Ordering::SeqCst) as u64;
        let id = tokio_trace::Id::from_u64(next);
        if span.name.contains("event") {
            self.events
                .lock()
                .unwrap()
                .insert(id.clone(), Event::new(span));
        } else {
            self.spans
                .lock()
                .unwrap()
                .insert(id.clone(), Span::new(self.current.id(), span));
        }
        id
    }

    fn record_debug(
        &self,
        span: &tokio_trace::Id,
        name: &tokio_trace::field::Field,
        value: &fmt::Debug,
    ) {
        if let Some(event) = self.events.lock().expect("mutex poisoned!").get_mut(span) {
            return event.record(name, format_args!("{:?}", value));
        };
        let mut spans = self.spans.lock().expect("mutex poisoned!");
        if let Some(span) = spans.get_mut(span) {
            span.record(name, format_args!("{:?}", value))
        }
    }

    fn add_follows_from(&self, _span: &tokio_trace::Id, _follows: tokio_trace::Id) {
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

    #[inline]
    fn exit(&self, _span: &tokio_trace::Id) {
        // TODO: unify stack with current span
        self.current.exit();
    }

    fn drop_span(&self, id: tokio_trace::Id) {
        if let Some(event) = self.events.lock().expect("mutex poisoned").remove(&id) {
            let mut stderr = self.stderr.lock();
            let indent = self.stack.lock().unwrap().len();
            self.print_indent(&mut stderr, indent).unwrap();
            write!(
                &mut stderr,
                "{timestamp} {level} {target} {message}",
                timestamp = humantime::format_rfc3339_seconds(SystemTime::now()),
                level = ColorLevel(event.level),
                target = &event.target,
                message = Style::new().bold().paint(event.message),
            )
            .unwrap();
            self.print_kvs(
                &mut stderr,
                event.kvs.iter().map(|&(ref k, ref v)| (k, v)),
                ", ",
            )
            .unwrap();
            write!(&mut stderr, "\n").unwrap();
        }
        // TODO: GC unneeded spans.
    }
}
