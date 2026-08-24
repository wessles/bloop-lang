use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

#[derive(Debug)]
pub(super) struct PositionalError {
    pub(super) m_error: Box<dyn Error>,
    m_line_num: usize,
    m_char_num: usize,
}

pub(super) struct PositionalErrorWithContext<'a> {
    m_error: PositionalError,
    m_program_path: Option<&'a str>,
    m_program_str: &'a str,
}

impl Debug for PositionalErrorWithContext<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl Error for PositionalErrorWithContext<'_> {}

impl<'a> Display for PositionalErrorWithContext<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let path = self.m_program_path.unwrap_or("program");
        let line_str = self.m_program_str.split('\n').nth(self.m_error.m_line_num - 1).unwrap();
        write!(
            f,
            "{}:{}:{}: {}\n{}\n{}^ here",
            path,
            self.m_error.m_line_num,
            self.m_error.m_char_num,
            self.m_error.m_error,
            line_str,
            " ".repeat(self.m_error.m_char_num - 1)
        )
    }
}

impl PositionalError {
    pub(super) fn new(error: Box<dyn Error>, line_num: usize, char_num: usize) -> Self {
        Self {
            m_error: error,
            m_line_num: line_num,
            m_char_num: char_num,
        }
    }

    pub(super) fn get_err<'a>(
        self,
        source: &'a str,
        path: Option<&'a str>,
    ) -> PositionalErrorWithContext<'a> {
        PositionalErrorWithContext {
            m_error: self,
            m_program_path: path,
            m_program_str: source,
        }
    }
}
