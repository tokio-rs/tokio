#![allow(missing_docs)]
use super::span::MockSpan;
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use tokio_trace::{field, Id, Metadata, Subscriber};

#[derive(Debug, Eq, PartialEq)]
struct ExpectEvent {
    // TODO: implement
}

#[derive(Debug, Eq, PartialEq)]
enum Expect {
    #[allow(dead_code)] // TODO: implement!
    Event(ExpectEvent),
    Enter(MockSpan),
    Exit(MockSpan),
    CloneSpan(MockSpan),
    DropSpan(MockSpan),
    Nothing,
}

struct SpanState {
    name: &'static str,
    refs: usize,
}

struct Running<F: Fn(&Metadata) -> bool> {
    spans: Mutex<HashMap<Id, SpanState>>,
    expected: Arc<Mutex<VecDeque<Expect>>>,
    ids: AtomicUsize,
    filter: F,
}

pub struct MockSubscriber<F: Fn(&Metadata) -> bool> {
    expected: VecDeque<Expect>,
    filter: F,
}

pub struct MockHandle(Arc<Mutex<VecDeque<Expect>>>);

pub fn mock() -> MockSubscriber<fn(&Metadata) -> bool> {
    MockSubscriber {
        expected: VecDeque::new(),
        filter: (|_: &Metadata| true) as for<'r, 's> fn(&'r Metadata<'s>) -> _,
    }
}

impl<F: Fn(&Metadata) -> bool> MockSubscriber<F> {
    pub fn enter(mut self, span: MockSpan) -> Self {
        self.expected.push_back(Expect::Enter(span));
        self
    }

    pub fn event(mut self) -> Self {
        // TODO: expect message/fields!
        self.expected.push_back(Expect::Event(ExpectEvent {}));
        self
    }

    pub fn exit(mut self, span: MockSpan) -> Self {
        self.expected.push_back(Expect::Exit(span));
        self
    }

    pub fn clone_span(mut self, span: MockSpan) -> Self {
        self.expected.push_back(Expect::CloneSpan(span));
        self
    }

    pub fn drop_span(mut self, span: MockSpan) -> Self {
        self.expected.push_back(Expect::DropSpan(span));
        self
    }

    pub fn done(mut self) -> Self {
        self.expected.push_back(Expect::Nothing);
        self
    }

    pub fn with_filter<G>(self, filter: G) -> MockSubscriber<G>
    where
        G: Fn(&Metadata) -> bool,
    {
        MockSubscriber {
            filter,
            expected: self.expected,
        }
    }

    pub fn run(self) -> impl Subscriber {
        let (subscriber, _) = self.run_with_handle();
        subscriber
    }

    pub fn run_with_handle(self) -> (impl Subscriber, MockHandle) {
        let expected = Arc::new(Mutex::new(self.expected));
        let handle = MockHandle(expected.clone());
        let subscriber = Running {
            spans: Mutex::new(HashMap::new()),
            expected,
            ids: AtomicUsize::new(0),
            filter: self.filter,
        };
        (subscriber, handle)
    }
}

impl<F: Fn(&Metadata) -> bool> Subscriber for Running<F> {
    fn enabled(&self, meta: &Metadata) -> bool {
        (self.filter)(meta)
    }

    fn record_debug(&self, _span: &Id, _field: &field::Field, _value: &fmt::Debug) {
        // TODO: it would be nice to be able to expect field values...
    }

    fn add_follows_from(&self, _span: &Id, _follows: Id) {
        // TODO: it should be possible to expect spans to follow from other spans
    }

    fn new_span(&self, meta: &Metadata) -> Id {
        let id = self.ids.fetch_add(1, Ordering::SeqCst);
        let id = Id::from_u64(id as u64);
        println!(
            "new_span: name={:?}; target={:?}; id={:?};",
            meta.name(),
            meta.target(),
            id
        );
        self.spans.lock().unwrap().insert(
            id.clone(),
            SpanState {
                name: meta.name(),
                refs: 1,
            },
        );
        id
    }

    fn enter(&self, id: &Id) {
        let spans = self.spans.lock().unwrap();
        if let Some(span) = spans.get(id) {
            println!("enter: {}; id={:?};", span.name, id);
            match self.expected.lock().unwrap().pop_front() {
                None => {}
                Some(Expect::Event(_)) => panic!(
                    "expected an event, but entered span {:?} instead",
                    span.name
                ),
                Some(Expect::Enter(ref expected_span)) => {
                    if let Some(name) = expected_span.name {
                        assert_eq!(name, span.name);
                    }
                    // TODO: expect fields
                }
                Some(Expect::Exit(ref expected_span)) => panic!(
                    "expected to exit {}, but entered span {:?} instead",
                    expected_span, span.name
                ),
                Some(Expect::CloneSpan(ref expected_span)) => panic!(
                    "expected to clone {}, but entered span {:?} instead",
                    expected_span, span.name
                ),
                Some(Expect::DropSpan(ref expected_span)) => panic!(
                    "expected to drop {}, but entered span {:?} instead",
                    expected_span, span.name
                ),
                Some(Expect::Nothing) => panic!(
                    "expected nothing else to happen, but entered span {:?}",
                    span.name,
                ),
            }
        };
    }

    fn exit(&self, id: &Id) {
        let spans = self.spans.lock().unwrap();
        let span = spans
            .get(id)
            .unwrap_or_else(|| panic!("no span for ID {:?}", id));
        println!("exit: {}; id={:?};", span.name, id);
        match self.expected.lock().unwrap().pop_front() {
            None => {}
            Some(Expect::Event(_)) => {
                panic!("expected an event, but exited span {:?} instead", span.name)
            }
            Some(Expect::Enter(ref expected_span)) => panic!(
                "expected to enter {}, but exited span {:?} instead",
                expected_span, span.name
            ),
            Some(Expect::Exit(ref expected_span)) => {
                if let Some(name) = expected_span.name {
                    assert_eq!(name, span.name);
                }
                // TODO: expect fields
            }
            Some(Expect::CloneSpan(ref expected_span)) => panic!(
                "expected to clone {}, but exited span {:?} instead",
                expected_span, span.name
            ),
            Some(Expect::DropSpan(ref expected_span)) => panic!(
                "expected to drop {}, but exited span {:?} instead",
                expected_span, span.name
            ),
            Some(Expect::Nothing) => panic!(
                "expected nothing else to happen, but exited span {:?}",
                span.name,
            ),
        };
    }

    fn clone_span(&self, id: &Id) -> Id {
        let name = self.spans.lock().unwrap().get_mut(id).map(|span| {
            let name = span.name;
            println!("clone_span: {}; id={:?}; refs={:?};", name, id, span.refs);
            span.refs += 1;
            name
        });
        if name.is_none() {
            println!("clone_span: id={:?};", id);
        }
        let mut expected = self.expected.lock().unwrap();
        let was_expected = if let Some(Expect::CloneSpan(ref span)) = expected.front() {
            assert_eq!(name, span.name);
            true
        } else {
            false
        };
        if was_expected {
            expected.pop_front();
        }
        id.clone()
    }

    fn drop_span(&self, id: Id) {
        let mut is_event = false;
        let name = if let Ok(mut spans) = self.spans.try_lock() {
            spans.get_mut(&id).map(|span| {
                let name = span.name;
                if name.contains("event") {
                    is_event = true;
                }
                println!("drop_span: {}; id={:?}; refs={:?};", name, id, span.refs);
                span.refs -= 1;
                name
            })
        } else {
            None
        };
        if name.is_none() {
            println!("drop_span: id={:?}", id);
        }
        if let Ok(mut expected) = self.expected.try_lock() {
            let was_expected = match expected.front() {
                Some(Expect::DropSpan(ref span)) => {
                    // Don't assert if this function was called while panicking,
                    // as failing the assertion can cause a double panic.
                    if !::std::thread::panicking() {
                        assert_eq!(name, span.name);
                    }
                    true
                }
                Some(Expect::Event(_)) => {
                    if !::std::thread::panicking() {
                        assert!(is_event);
                    }
                    true
                }
                _ => false,
            };
            if was_expected {
                expected.pop_front();
            }
        }
    }
}

impl MockHandle {
    pub fn assert_finished(&self) {
        if let Ok(ref expected) = self.0.lock() {
            assert!(
                !expected.iter().any(|thing| thing != &Expect::Nothing),
                "more notifications expected: {:?}",
                **expected
            );
        }
    }
}
