//! Searchable command palette.

// ─── Command palette ─────────────────────────────────────────────────────────

/// A named, searchable command.
pub struct Command {
    /// Unique identifier.
    pub id: String,
    /// Display label shown in the palette.
    pub label: String,
    /// Optional keyboard shortcut hint displayed alongside the label.
    pub shortcut: Option<String>,
    /// Action to invoke when the command is selected.
    pub action: Box<dyn Fn() + Send + Sync>,
}

/// A searchable registry of [`Command`]s.
///
/// Commands are registered via [`CommandPalette::register`] and searched
/// via [`CommandPalette::search`] using a simple fuzzy-match algorithm
/// (all query characters must appear in the label in order, case-insensitive).
pub struct CommandPalette {
    commands: Vec<Command>,
}

impl CommandPalette {
    /// Create an empty [`CommandPalette`].
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Register a command.
    pub fn register(
        &mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        action: impl Fn() + Send + Sync + 'static,
    ) {
        self.commands.push(Command {
            id: id.into(),
            label: label.into(),
            shortcut: None,
            action: Box::new(action),
        });
    }

    /// Register a command with an optional keyboard shortcut hint.
    pub fn register_with_shortcut(
        &mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        shortcut: Option<String>,
        action: impl Fn() + Send + Sync + 'static,
    ) {
        self.commands.push(Command {
            id: id.into(),
            label: label.into(),
            shortcut,
            action: Box::new(action),
        });
    }

    /// Search for commands whose labels fuzzy-match `query`.
    ///
    /// The match is case-insensitive and requires that every character in
    /// `query` appear in `label` in order (subsequence matching).
    pub fn search(&self, query: &str) -> Vec<&Command> {
        let query_lc = query.to_lowercase();
        self.commands
            .iter()
            .filter(|cmd| {
                let label_lc = cmd.label.to_lowercase();
                let mut q_iter = query_lc.chars();
                let mut current = q_iter.next();
                for ch in label_lc.chars() {
                    if current == Some(ch) {
                        current = q_iter.next();
                    }
                    if current.is_none() {
                        return true;
                    }
                }
                current.is_none()
            })
            .collect()
    }

    /// The number of registered commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Returns `true` if no commands are registered.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}
