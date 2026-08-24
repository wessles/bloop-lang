/// Up/down-arrow "recall a previous submission" behavior shared by the
/// native terminal REPL (`terminal_repl.rs`) and the browser REPL
/// (`wasm_repl.rs`), so both present the same history semantics.
pub(crate) struct History {
    entries: Vec<String>,
    // How far back from the newest entry we're currently browsing; `None`
    // means "not browsing, showing live input".
    idx: Option<usize>,
}

impl History {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            idx: None,
        }
    }

    /// Records a submitted line and stops browsing (back to live input).
    pub(crate) fn push(&mut self, line: String) {
        self.entries.push(line);
        self.idx = None;
    }

    /// Recalls an older entry. Returns `None` (leave the input alone) if
    /// there's no history at all; otherwise the entry to show.
    pub(crate) fn up(&mut self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        self.idx = Some(match self.idx {
            Some(idx) if idx + 1 < self.entries.len() => idx + 1,
            Some(idx) => idx,
            None => 0,
        });
        self.current()
    }

    /// Recalls a newer entry, or clears back to live input once already at
    /// the newest one. Returns `None` (leave the input alone) only if
    /// there's no history at all.
    pub(crate) fn down(&mut self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        self.idx = match self.idx {
            Some(0) | None => None,
            Some(idx) => Some(idx - 1),
        };
        Some(self.current().unwrap_or_default())
    }

    fn current(&self) -> Option<String> {
        self.idx
            .map(|idx| self.entries[self.entries.len() - 1 - idx].clone())
    }
}
