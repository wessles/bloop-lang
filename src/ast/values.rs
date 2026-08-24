use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    String(String),
    Integer(i64),
    Double(f64),
    Boolean(bool),
}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Value::String(s) => {
                0u8.hash(state);
                s.hash(state);
            }
            Value::Integer(i) => {
                1u8.hash(state);
                i.hash(state);
            }
            Value::Double(d) => {
                2u8.hash(state);
                d.to_bits().hash(state);
            }
            Value::Boolean(b) => {
                3u8.hash(state);
                b.hash(state);
            }
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::Integer(i) => write!(f, "{}", i),
            Value::Double(d) => write!(f, "{}", d),
            Value::Boolean(b) => write!(f, "{}", b),
        }
    }
}
