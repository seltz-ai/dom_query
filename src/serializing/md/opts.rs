use std::borrow::Cow;

use super::text_utils::linebreak;

use super::constants::MAX_LIST_NUMBER;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct EmphasisScope(u8);

impl EmphasisScope {
    pub const BOLD: Self = Self(1 << 0);
    pub const ITALIC: Self = Self(1 << 1);
    pub const STRIKE: Self = Self(1 << 2);

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[must_use]
    pub const fn insert(mut self, other: Self) -> Self {
        self.0 |= other.0;
        self
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The markers that apply all of this scope's emphases at once;
    /// strikethrough is not applied across blocks.
    pub const fn markers(self) -> &'static str {
        match (self.contains(Self::BOLD), self.contains(Self::ITALIC)) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            (false, false) => "",
        }
    }
}

impl From<&str> for EmphasisScope {
    fn from(marker: &str) -> Self {
        match marker {
            // TODO: support "_"
            "*" => Self::ITALIC,
            "~~" => Self::STRIKE,
            _ => Self::BOLD,
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Default, Clone, Copy)]
pub struct FormatOpts {
    pub include_node: bool,
    pub skip_escape: bool,
    pub inline: bool,
    /// Whether to skip Markdown formatting syntax and render plain text.
    pub skip_md: bool,
    /// Enables formatting rules specific to table cells:
    /// 1. Prevents standard line breaks.
    /// 2. Replaces line breaks with `<br>` tags.
    pub table_cell: bool,
    /// Enables formatting rules for list item
    pub list_item: bool,
    /// One line, as in a heading: blocks and line breaks become spaces.
    pub single_line: bool,
    /// Inside a link's text, where a nested link is written as its text.
    pub in_link: bool,
    pub offset: usize,
    pub emphasis_scope: EmphasisScope,
    /// Emphases holding blocks, which Markdown cannot span: each text block
    /// inside applies them itself.
    pub block_emphasis: EmphasisScope,
}

impl FormatOpts {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn include_node(mut self) -> Self {
        self.include_node = true;
        self
    }

    pub const fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    pub const fn skip_escape(mut self) -> Self {
        self.skip_escape = true;
        self
    }

    pub const fn table_cell(mut self) -> Self {
        self.table_cell = true;
        self
    }

    pub const fn list_item(mut self) -> Self {
        self.list_item = true;
        self
    }

    pub const fn inline(mut self) -> Self {
        self.inline = true;
        self
    }
    pub const fn single_line(mut self) -> Self {
        self.single_line = true;
        self
    }
    pub const fn in_link(mut self) -> Self {
        self.in_link = true;
        self
    }
    pub const fn skip_md(mut self) -> Self {
        self.skip_md = true;
        self
    }

    pub const fn emphasis_scope(mut self, scope: EmphasisScope) -> Self {
        self.emphasis_scope = self.emphasis_scope.insert(scope);
        self
    }

    pub const fn block_emphasis(mut self, scope: EmphasisScope) -> Self {
        self.block_emphasis = self.block_emphasis.insert(scope);
        self
    }

    /// Options for the inline content of a text block that applies the
    /// emphases spanning blocks around it.
    pub const fn applying_block_emphasis(mut self) -> Self {
        self.emphasis_scope = self.emphasis_scope.insert(self.block_emphasis);
        self.block_emphasis = EmphasisScope(0);
        self
    }
}

#[derive(Debug, Clone, Copy)]
/// Represents the kind of list being serialized.
pub enum ListKind {
    Ul,
    Ol(u32),
}

impl ListKind {
    /// The width of the marker: content nested in an item is indented to it,
    /// as more would turn an item that starts inline, or a line after a blank
    /// one, into a code block.
    fn offset(&self) -> usize {
        match *self {
            ListKind::Ol(n) => {
                let digits = if n == 0 { 1 } else { n.ilog10() + 1 };
                digits as usize + 2
            }
            ListKind::Ul => 2,
        }
    }
}

/// Context for a list being serialized.
pub struct ListContext {
    pub opts: FormatOpts,
    pub kind: ListKind,
    list_indent: usize,
    /// Whether this list writes the other marker, `+` for `-` or `)` for `.`,
    /// as a list right after a list with the same marker would continue it.
    other_marker: bool,
}

impl ListContext {
    pub fn new(opts: FormatOpts, kind: ListKind) -> Self {
        let mut list_opts = opts.offset(opts.offset + kind.offset());
        // a list written as a node must not make its items write theirs
        list_opts.include_node = false;
        Self {
            opts: list_opts,
            kind,
            list_indent: opts.offset,
            other_marker: false,
        }
    }

    pub const fn other_marker(mut self, other: bool) -> Self {
        self.other_marker = other;
        self
    }

    pub fn list_indent(&self) -> String {
        " ".repeat(self.list_indent)
    }

    pub const fn linebreak(&self) -> &str {
        linebreak(self.opts.table_cell)
    }

    const fn ul_prefix(&self) -> &'static str {
        if self.opts.table_cell == self.other_marker {
            "- "
        } else {
            "+ "
        }
    }

    pub fn prefix(&self) -> Cow<'static, str> {
        match self.kind {
            ListKind::Ul => Cow::Borrowed(self.ul_prefix()),
            ListKind::Ol(n) if self.other_marker => Cow::Owned(format!("{n}) ")),
            ListKind::Ol(n) => Cow::Owned(format!("{n}. ")),
        }
    }

    pub fn advance_ol_number(&mut self) {
        if let ListKind::Ol(n) = self.kind {
            let next_num = n.saturating_add(1).min(MAX_LIST_NUMBER);
            self.kind = ListKind::Ol(next_num);
            // update offset each time we advance the number
            self.opts = self.opts.offset(self.list_indent + self.kind.offset());
        }
    }
}
