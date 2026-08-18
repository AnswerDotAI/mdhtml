use std::fmt;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attr {
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub pairs: Vec<(String, String)>,
}

impl Attr {
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.classes.is_empty() && self.pairs.is_empty()
    }
    pub fn with_class(class: impl Into<String>) -> Self {
        let mut a = Self::default();
        a.push_class(class);
        a
    }
    pub fn push_class(&mut self, class: impl Into<String>) {
        let class = class.into();
        if !class.is_empty() && !self.classes.iter().any(|c| c == &class) {
            self.classes.push(class);
        }
    }
    pub fn set_pair(&mut self, key: impl Into<String>, val: impl Into<String>) {
        let key = key.into();
        let val = val.into();
        if key == "id" {
            self.id = Some(val);
            return;
        }
        if key == "class" {
            for c in val.split_whitespace() {
                self.push_class(c);
            }
            return;
        }
        if let Some((_, v)) = self.pairs.iter_mut().find(|(k, _)| k == &key) {
            *v = val;
        } else {
            self.pairs.push((key, val));
        }
    }
    pub fn merge(&mut self, other: &Attr) {
        if let Some(id) = &other.id {
            self.id = Some(id.clone());
        }
        for class in &other.classes {
            self.push_class(class.clone());
        }
        for (k, v) in &other.pairs {
            self.set_pair(k.clone(), v.clone());
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRef {
    pub url: String,
    pub title: Option<String>,
    pub attrs: Attr,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub footnotes: Vec<Footnote>,
    /// Parse-time findings, e.g. constructs left unclosed at end of input.
    pub warnings: Vec<String>,
    /// Frontmatter metadata in source order; empty unless the document opened
    /// with a well-shaped block and `Options::frontmatter` was set.
    pub meta: Vec<(String, String)>,
}

/// A template token found between the tags of a raw HTML block: `start..end`
/// index its `Block::Html`'s `raw` text.
#[derive(Clone, Debug, PartialEq)]
pub struct HtmlToken {
    pub start: usize,
    pub end: usize,
    pub syntax: String,
    pub body: String,
    pub kind: crate::template::TokenKind,
    pub name: String,
    /// The token sits between table rows (see `template::html_tokens`).
    pub row: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Footnote {
    pub label: String,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    pub attrs: Attr,
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DefinitionItem {
    pub terms: Vec<Vec<Inline>>,
    pub definitions: Vec<Vec<Inline>>,
}

pub type TableRow = TableRowData<Vec<Inline>>;
pub type TableCell = TableCellData<Vec<Inline>>;

#[derive(Clone, Debug, PartialEq)]
pub struct TableRowData<C> {
    pub attrs: Attr,
    pub cells: Vec<TableCellData<C>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableCellData<C> {
    pub attrs: Attr,
    pub align: Align,
    pub content: C,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Paragraph {
        attrs: Attr,
        children: Vec<Inline>,
    },
    Heading {
        level: u8,
        attrs: Attr,
        children: Vec<Inline>,
    },
    BlockQuote {
        attrs: Attr,
        children: Vec<Block>,
    },
    List {
        attrs: Attr,
        ordered: bool,
        start: usize,
        tight: bool,
        items: Vec<ListItem>,
    },
    DefinitionList {
        attrs: Attr,
        items: Vec<DefinitionItem>,
    },
    CodeBlock {
        attrs: Attr,
        info: String,
        lang: Option<String>,
        text: String,
    },
    Html {
        raw: String,
        tokens: Vec<HtmlToken>,
    },
    ThematicBreak {
        attrs: Attr,
    },
    Table {
        attrs: Attr,
        aligns: Vec<Align>,
        head: Vec<TableRow>,
        rows: Vec<TableRow>,
        foot: Vec<TableRow>,
        caption: Vec<Inline>,
        /// Range markers between body rows: `(index, marker)` places the marker
        /// before `rows[index]` (`rows.len()` = after the last row).
        row_tokens: Vec<(usize, Inline)>,
    },
    Div {
        attrs: Attr,
        children: Vec<Block>,
    },
    Math {
        attrs: Attr,
        display: bool,
        tex: String,
    },
    Figure {
        attrs: Attr,
        caption: Vec<Inline>,
        image: Inline,
    },
    TemplateToken {
        syntax: String,
        source: String,
        body: String,
        kind: crate::template::TokenKind,
        name: String,
    },

    Raw {
        format: String,
        text: String,
    },
    /// An active-code block (`{lang}` fence), carried as
    /// `<script type="text/<lang>-block">`; executed only by `instantiate`.
    Script {
        lang: String,
        text: String,
    },
}

impl Block {
    pub fn attrs_mut(&mut self) -> Option<&mut Attr> {
        match self {
            Block::Paragraph { attrs, .. }
            | Block::Heading { attrs, .. }
            | Block::BlockQuote { attrs, .. }
            | Block::List { attrs, .. }
            | Block::DefinitionList { attrs, .. }
            | Block::CodeBlock { attrs, .. }
            | Block::ThematicBreak { attrs, .. }
            | Block::Table { attrs, .. }
            | Block::Div { attrs, .. }
            | Block::Math { attrs, .. }
            | Block::Figure { attrs, .. } => Some(attrs),
            Block::Html { .. } | Block::TemplateToken { .. } | Block::Raw { .. } | Block::Script { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String),
    SoftBreak,
    HardBreak,
    Emph { attrs: Attr, children: Vec<Inline> },
    Strong { attrs: Attr, children: Vec<Inline> },
    Strike { attrs: Attr, children: Vec<Inline> },
    Superscript { attrs: Attr, text: String },
    Subscript { attrs: Attr, text: String },
    Highlight { attrs: Attr, children: Vec<Inline> },
    Code { attrs: Attr, text: String },
    Link { attrs: Attr, children: Vec<Inline>, url: String, title: Option<String> },
    Image { attrs: Attr, alt: Vec<Inline>, url: String, title: Option<String> },
    Autolink { url: String, text: String, email: bool },
    Html(String),
    TemplateToken { syntax: String, source: String, body: String, kind: crate::template::TokenKind, name: String },
    Math { attrs: Attr, display: bool, tex: String },
    FootnoteRef { label: String },
    Note { children: Vec<Inline> },
    Span { attrs: Attr, children: Vec<Inline> },
    Raw { format: String, text: String },
}

impl Inline {
    pub fn attrs_mut(&mut self) -> Option<&mut Attr> {
        match self {
            Inline::Emph { attrs, .. }
            | Inline::Strong { attrs, .. }
            | Inline::Strike { attrs, .. }
            | Inline::Superscript { attrs, .. }
            | Inline::Subscript { attrs, .. }
            | Inline::Highlight { attrs, .. }
            | Inline::Code { attrs, .. }
            | Inline::Link { attrs, .. }
            | Inline::Image { attrs, .. }
            | Inline::Math { attrs, .. }
            | Inline::Span { attrs, .. } => Some(attrs),
            _ => None,
        }
    }
}

impl fmt::Display for Align {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Align::None => "",
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        })
    }
}
