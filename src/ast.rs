use crate::Diagnostic;
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
        let mut attr = Self::default();
        attr.push_class(class);
        attr
    }

    pub fn push_class(&mut self, class: impl Into<String>) {
        let class = class.into();
        if !class.is_empty() && !self.classes.iter().any(|item| item == &class) {
            self.classes.push(class)
        }
    }

    pub fn set_pair(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if key == "id" {
            self.id = Some(value);
            return;
        }
        if key == "class" {
            for class in value.split_whitespace() {
                self.push_class(class)
            }
            return;
        }
        if let Some((_, current)) = self.pairs.iter_mut().find(|(name, _)| name == &key) { *current = value } else { self.pairs.push((key, value)) }
    }

    pub fn merge(&mut self, other: &Attr) {
        if let Some(id) = &other.id {
            self.id = Some(id.clone())
        }
        for class in &other.classes {
            self.push_class(class.clone())
        }
        for (key, value) in &other.pairs {
            self.set_pair(key.clone(), value.clone())
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

/// Template token classification. Unknown syntax remains explicit so policy
/// stays in the consuming renderer rather than the parser or shared IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Var,
    Open,
    OpenInverted,
    Close,
    Unknown,
}

impl TokenKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Var => "var",
            Self::Open | Self::OpenInverted => "open",
            Self::Close => "close",
            Self::Unknown => "unknown",
        }
    }
    pub fn inverted(self) -> bool {
        self == Self::OpenInverted
    }
    pub fn is_marker(self) -> bool {
        matches!(self, Self::Open | Self::OpenInverted | Self::Close)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub footnotes: Vec<Footnote>,
    pub diagnostics: Vec<Diagnostic>,
    /// Frontmatter metadata in source order.
    pub meta: Vec<(String, String)>,
}

/// A template token found between tags of a raw HTML block. `start..end`
/// indexes the containing `Block::Html`'s `raw` text.
#[derive(Clone, Debug, PartialEq)]
pub struct HtmlToken {
    pub start: usize,
    pub end: usize,
    pub syntax: String,
    pub body: String,
    pub kind: TokenKind,
    pub name: String,
    pub row: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Footnote {
    pub label: String,
    pub blocks: Vec<Block>,
}

/// A semantic instruction carried by MDHTML, such as a MediaWiki template
/// transclusion. Arguments retain their parsed inline structure.
#[derive(Clone, Debug, PartialEq)]
pub struct Operation {
    pub syntax: String,
    pub action: String,
    pub name: String,
    pub args: Vec<OperationArg>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OperationArg {
    pub name: Option<String>,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    pub attrs: Attr,
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DefinitionTerm {
    pub attrs: Attr,
    pub inlines: Vec<Inline>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DefinitionItem {
    pub terms: Vec<DefinitionTerm>,
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
        kind: TokenKind,
        name: String,
    },
    Raw {
        format: String,
        text: String,
    },
    Script {
        lang: String,
        text: String,
    },
}

impl Block {
    pub fn attrs_mut(&mut self) -> Option<&mut Attr> {
        match self {
            Self::Paragraph { attrs, .. }
            | Self::Heading { attrs, .. }
            | Self::BlockQuote { attrs, .. }
            | Self::List { attrs, .. }
            | Self::DefinitionList { attrs, .. }
            | Self::CodeBlock { attrs, .. }
            | Self::ThematicBreak { attrs }
            | Self::Table { attrs, .. }
            | Self::Div { attrs, .. }
            | Self::Math { attrs, .. }
            | Self::Figure { attrs, .. } => Some(attrs),
            Self::Html { .. } | Self::TemplateToken { .. } | Self::Raw { .. } | Self::Script { .. } => None,
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
    Operation(Operation),
    TemplateToken { syntax: String, source: String, body: String, kind: TokenKind, name: String },
    Math { attrs: Attr, display: bool, tex: String },
    FootnoteRef { label: String },
    Note { children: Vec<Inline> },
    Span { attrs: Attr, children: Vec<Inline> },
    Raw { format: String, text: String },
}

impl Inline {
    pub fn attrs_mut(&mut self) -> Option<&mut Attr> {
        match self {
            Self::Emph { attrs, .. }
            | Self::Strong { attrs, .. }
            | Self::Strike { attrs, .. }
            | Self::Superscript { attrs, .. }
            | Self::Subscript { attrs, .. }
            | Self::Highlight { attrs, .. }
            | Self::Code { attrs, .. }
            | Self::Link { attrs, .. }
            | Self::Image { attrs, .. }
            | Self::Math { attrs, .. }
            | Self::Span { attrs, .. } => Some(attrs),
            _ => None,
        }
    }
}
