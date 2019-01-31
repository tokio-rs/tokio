use tokio_trace::{
    callsite::Callsite,
    field::{self, Field, Record, Value},
};

use std::{collections::HashMap, fmt};

#[derive(Default, Debug, Eq, PartialEq)]
pub struct Expect {
    fields: HashMap<String, MockValue>,
    only: bool,
}

#[derive(Debug)]
pub struct MockField {
    name: String,
    value: MockValue,
}

#[derive(Debug, Eq, PartialEq)]
pub enum MockValue {
    I64(i64),
    U64(u64),
    Bool(bool),
    Str(String),
    Debug(String),
    Any,
}

pub fn mock<K>(name: K) -> MockField
where
    String: From<K>,
{
    MockField {
        name: name.into(),
        value: MockValue::Any,
    }
}

impl MockField {
    /// Expect a field with the given name and value.
    pub fn with_value(self, value: &Value) -> Self {
        Self {
            value: MockValue::from(value),
            ..self
        }
    }

    pub fn and(self, other: MockField) -> Expect {
        Expect {
            fields: HashMap::new(),
            only: false,
        }
        .and(self)
        .and(other)
    }

    pub fn only(self) -> Expect {
        Expect {
            fields: HashMap::new(),
            only: true,
        }
        .and(self)
    }
}

impl Into<Expect> for MockField {
    fn into(self) -> Expect {
        Expect {
            fields: HashMap::new(),
            only: false,
        }
        .and(self)
    }
}

impl Expect {
    pub fn and(mut self, field: MockField) -> Self {
        self.fields.insert(field.name, field.value);
        self
    }

    /// Indicates that no fields other than those specified should be expected.
    pub fn only(self) -> Self {
        Self { only: true, ..self }
    }

    fn compare_or_panic(&mut self, name: &str, value: &Value, ctx: &str) {
        let value = value.into();
        match self.fields.remove(name) {
            Some(MockValue::Any) => {}
            Some(expected) => assert!(
                expected == value,
                "\nexpected `{}` to contain:\n\t`{}{}`\nbut got:\n\t`{}{}`",
                ctx,
                name,
                expected,
                name,
                value
            ),
            None if self.only => panic!(
                "\nexpected `{}` to contain only:\n\t`{}`\nbut got:\n\t`{}{}`",
                ctx, self, name, value
            ),
            _ => {}
        }
    }

    pub fn checker<'a>(&'a mut self, ctx: String) -> CheckRecorder<'a> {
        CheckRecorder { expect: self, ctx }
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

impl fmt::Display for MockValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MockValue::I64(v) => write!(f, ": i64 = {:?}", v),
            MockValue::U64(v) => write!(f, ": u64 = {:?}", v),
            MockValue::Bool(v) => write!(f, ": bool = {:?}", v),
            MockValue::Str(v) => write!(f, ": &str = {:?}", v),
            MockValue::Debug(v) => write!(f, ": &fmt::Debug = {:?}", v),
            MockValue::Any => write!(f, ": _ = _"),
        }
    }
}

pub struct CheckRecorder<'a> {
    expect: &'a mut Expect,
    ctx: String,
}

impl<'a> Record for CheckRecorder<'a> {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.expect
            .compare_or_panic(field.name(), &value, &self.ctx[..])
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.expect
            .compare_or_panic(field.name(), &value, &self.ctx[..])
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.expect
            .compare_or_panic(field.name(), &value, &self.ctx[..])
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.expect
            .compare_or_panic(field.name(), &value, &self.ctx[..])
    }

    fn record_debug(&mut self, field: &Field, value: &fmt::Debug) {
        self.expect
            .compare_or_panic(field.name(), &field::debug(value), &self.ctx)
    }
}

impl<'a> CheckRecorder<'a> {
    pub fn finish(self) {
        assert!(
            self.expect.fields.is_empty(),
            "{}missing {}",
            self.expect,
            self.ctx
        );
    }
}

impl<'a> From<&'a Value> for MockValue {
    fn from(value: &'a Value) -> Self {
        struct MockValueBuilder {
            value: Option<MockValue>,
        }

        impl Record for MockValueBuilder {
            fn record_i64(&mut self, _: &Field, value: i64) {
                self.value = Some(MockValue::I64(value));
            }

            fn record_u64(&mut self, _: &Field, value: u64) {
                self.value = Some(MockValue::U64(value));
            }

            fn record_bool(&mut self, _: &Field, value: bool) {
                self.value = Some(MockValue::Bool(value));
            }

            fn record_str(&mut self, _: &Field, value: &str) {
                self.value = Some(MockValue::Str(value.to_owned()));
            }

            fn record_debug(&mut self, _: &Field, value: &fmt::Debug) {
                self.value = Some(MockValue::Debug(format!("{:?}", value)));
            }
        }

        let fake_field = callsite!(name: "fake", fields: fake_field)
            .metadata()
            .fields()
            .field("fake_field")
            .unwrap();
        let mut builder = MockValueBuilder { value: None };
        value.record(&fake_field, &mut builder);
        builder
            .value
            .expect("finish called before a value was recorded")
    }
}

impl fmt::Display for Expect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "fields ")?;
        let entries = self
            .fields
            .iter()
            .map(|(k, v)| (field::display(k), field::display(v)));
        f.debug_map().entries(entries).finish()
    }
}
