use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Hash, PartialEq)]
pub(crate) enum Type {
    Unknown,
    TVar(usize),
    I64,
    F64,
    String,
    Boolean,
    Function(Vec<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Unit,
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::I64 => write!(f, "i64"),
            Type::F64 => write!(f, "f64"),
            Type::String => write!(f, "string"),
            Type::Function(input_types, output_type) => {
                write!(
                    f,
                    "fn({}) -> {}",
                    input_types
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<String>>()
                        .join(", "),
                    output_type
                )
            }
            Type::Boolean => write!(f, "bool"),
            Type::Tuple(types) => write!(
                f,
                "({})",
                types
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            Type::Unit => write!(f, "unit"),
            Type::Unknown => write!(f, "**unknown**"),
            Type::TVar(id) => write!(f, "?t{}", id),
        }
    }
}
