use crate::Diagnostic;
use std::fmt;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attr { pub id: Option<String>, pub classes: Vec<String>, pub pairs: Vec<(String, String)> }

impl Attr {
    pub fn is_empty(&self) -> bool { self.id.is_none() && self.classes.is_empty() && self.pairs.is_empty() }

    pub fn with_class(class: impl Into<String>) -> Self {
        let mut attr = Self::default();
        attr.push_class(class);
        attr
    }

    pub fn push_class(&mut self, class: impl Into<String>) {
        let class = class.into();
        if !class.is_empty() && !self.classes.iter().any(|item| item == &class) { self.classes.push(class) }
    }

    pub fn set_pair(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if key == "id" { self.id = Some(value); return; }
        if key == "class" { for class in value.split_whitespace() { self.push_class(class) } return; }
        if let Some((_, current)) = self.pairs.iter_mut().find(|(name, _)| name == &key) { *current = value } else { self.pairs.push((key, value)) }
    }

    pub fn merge(&mut self, other: &Attr) {
        if let Some(id) = &other.id { self.id = Some(id.clone()) }
        for class in &other.classes { self.push_class(class.clone()) }
        for (key, value) in &other.pairs { self.set_pair(key.clone(), value.clone()) }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align { #[default] None, Left, Center, Right }

impl fmt::Display for Align {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self { Align::None => "", Align::Left => "left", Align::Center => "center", Align::Right => "right" })
    }
}

/// Template token classification. Unknown syntax remains explicit so policy
/// stays in the consuming renderer rather than the parser or shared IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind { Var, Open, OpenInverted, Close, Unknown }

impl TokenKind {
    pub fn as_str(self) -> &'static str {
        match self { Self::Var => "var", Self::Open | Self::OpenInverted => "open", Self::Close => "close", Self::Unknown => "unknown" }
    }
    pub fn inverted(self) -> bool { self == Self::OpenInverted }
    pub fn is_marker(self) -> bool { matches!(self, Self::Open | Self::OpenInverted | Self::Close) }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub footnotes: Vec<Footnote>,
    pub diagnostics: Vec<Diagnostic>,
    /// Complete recognized frontmatter, including its fences.
    pub frontmatter: String,
}

impl Document {
    pub fn meta(&self) -> Vec<(String, String)> { crate::frontmatter::extract(&self.frontmatter).map(|(meta, _)| meta).unwrap_or_default() }

    /// Rewrite nonempty link and image URLs in actual document content, not instruction operands.
    pub fn map_urls<E>(&mut self, rewrite: &mut impl FnMut(&str, bool) -> Result<Option<String>, E>) -> Result<(), E> {
        self.transform_inlines(&mut |inline| {
            let (url, image) = match inline { Inline::Link { url, .. } => (url, false), Inline::Image { url, .. } => (url, true), _ => return Ok(()) };
            if !url.is_empty() && let Some(replacement) = rewrite(url, image)? { *url = replacement; }
            Ok(())
        })
    }

    /// Transform visible inline content postorder; newly inserted content is not revisited.
    pub(crate) fn transform_inlines<E>(&mut self, transform: &mut impl FnMut(&mut Inline) -> Result<(), E>) -> Result<(), E> {
        fn inlines<E>(items: &mut [Inline], transform: &mut impl FnMut(&mut Inline) -> Result<(), E>) -> Result<(), E> {
            for inline in items {
                match inline {
                    Inline::Span { children, .. } | Inline::Emph { children, .. } | Inline::Strong { children, .. } | Inline::Strike { children, .. }
                    | Inline::Superscript { children, .. } | Inline::Subscript { children, .. } | Inline::Highlight { children, .. }
                    | Inline::Link { children, .. } | Inline::Element { children, .. } => inlines(children, transform)?, _ => {}
                }
                transform(inline)?;
            }
            Ok(())
        }
        fn blocks<E>(items: &mut [Block], transform: &mut impl FnMut(&mut Inline) -> Result<(), E>) -> Result<(), E> {
            for block in items {
                match block {
                    Block::Paragraph { children, .. } | Block::Heading { children, .. } => inlines(children, transform)?,
                    Block::Panel(panel) => {
                        if let Some(title) = &mut panel.title { inlines(&mut title.children, transform)?; }
                        blocks(&mut panel.body, transform)?;
                    }
                    Block::Figure { caption, body, .. } => {
                        inlines(caption, transform)?;
                        blocks(body, transform)?;
                    }
                    Block::Table { caption, head, rows, foot, .. } => {
                        inlines(caption, transform)?;
                        for entry in head.iter_mut().chain(rows).chain(foot) {
                            if let TableEntry::Row(row) = entry { for cell in &mut row.cells { blocks(&mut cell.content, transform)?; } }
                        }
                    }
                    Block::DefinitionList { items, .. } => {
                        for item in items {
                            for term in &mut item.terms { inlines(&mut term.inlines, transform)?; }
                            for definition in &mut item.definitions { blocks(definition, transform)?; }
                        }
                    }
                    Block::List { items, .. } => { for item in items { blocks(&mut item.blocks, transform)?; } }
                    Block::Element { children, .. } | Block::BlockQuote { children, .. } | Block::Div { children, .. } => blocks(children, transform)?,
                    _ => {}
                }
            }
            Ok(())
        }
        blocks(&mut self.blocks, transform)?;
        for note in &mut self.footnotes { blocks(&mut note.blocks, transform)?; }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Footnote { pub label: String, pub blocks: Vec<Block> }

/// A semantic instruction carried by MDHTML, such as a MediaWiki template
/// transclusion. Arguments retain their parsed inline structure.
#[derive(Clone, Debug, PartialEq)]
pub struct Operation { pub syntax: String, pub action: String, pub name: String, pub args: Vec<OperationArg>, pub content: Option<usize> }

impl Operation {
    pub fn token(syntax: String, body: String, kind: TokenKind, name: String) -> Self {
        let action = match kind {
            TokenKind::Var => "value", TokenKind::Open => "section", TokenKind::OpenInverted => "inverted", TokenKind::Close => "end",
            TokenKind::Unknown => "unknown"
        };
        Self { syntax, action: action.into(), name: if kind == TokenKind::Unknown { body.trim().into() } else { name }, args: Vec::new(), content: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OperationArg { pub name: Option<String>, pub content: Vec<Block> }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Disclosure { #[default] Fixed, Open, Closed }

#[derive(Clone, Debug, PartialEq)]
pub struct PanelTitle { pub attrs: Attr, pub children: Vec<Inline> }

#[derive(Clone, Debug, PartialEq)]
pub struct Panel { pub attrs: Attr, pub kind: String, pub disclosure: Disclosure, pub title: Option<PanelTitle>, pub body: Vec<Block> }

impl Panel {
    pub fn from_attrs(attrs: &Attr) -> Option<Self> {
        let get = |key| attrs.pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        let callout = get("data-callout").or_else(|| attrs.classes.iter().find_map(|c| c.strip_prefix("callout-").filter(|k| !k.is_empty())));
        let details = attrs.classes.iter().any(|c| c == "details");
        if get("data-panel").is_none() && callout.is_none() && !details { return None; }
        let disclosure = match get("data-disclosure").or_else(|| match get("collapse") {
            Some("true") => Some("closed"), Some("false") => Some("open"), _ => None
        }) {
            Some("open") => Disclosure::Open, Some("closed") => Disclosure::Closed,
            None if details => { if get("open").is_some() { Disclosure::Open } else { Disclosure::Closed } } _ => Disclosure::Fixed
        };
        let kind = callout.unwrap_or("").into();
        let mut attrs = attrs.clone();
        attrs.classes.retain(|c| c != "details" && !c.starts_with("callout-"));
        attrs.pairs.retain(|(k, _)| !matches!(k.as_str(), "collapse" | "open" | "data-panel" | "data-callout" | "data-disclosure"));
        Some(Self { attrs, kind, disclosure, title: None, body: Vec::new() })
    }

    pub fn label(&self) -> String {
        let mut chars = self.kind.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(), None if self.disclosure != Disclosure::Fixed => "Details".into(), None => String::new()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference { pub target: String, pub tokens: Vec<String>, pub prefix: String }

#[derive(Clone, Debug, PartialEq)]
pub struct AssignedNumber { pub display: String, pub reference: String, pub leaf: String }

#[derive(Clone, Debug, PartialEq)]
pub struct ListItem { pub attrs: Attr, pub checked: Option<bool>, pub blocks: Vec<Block> }

#[derive(Clone, Debug, PartialEq)]
pub struct DefinitionTerm { pub attrs: Attr, pub inlines: Vec<Inline> }

#[derive(Clone, Debug, PartialEq)]
pub struct DefinitionItem { pub terms: Vec<DefinitionTerm>, pub definitions: Vec<Vec<Block>> }

pub type TableRow = TableRowData<Vec<Block>>;
pub type TableCell = TableCellData<Vec<Block>>;

#[derive(Clone, Debug, PartialEq)]
pub struct TableRowData<C> { pub attrs: Attr, pub cells: Vec<TableCellData<C>> }

#[derive(Clone, Debug, PartialEq)]
pub struct TableCellData<C> { pub attrs: Attr, pub align: Align, pub content: C }

#[derive(Clone, Debug, PartialEq)]
pub enum TableEntry { Row(TableRow), Operation(Operation) }

pub fn table_columns(aligns: &[Align], head: &[TableEntry], rows: &[TableEntry], foot: &[TableEntry]) -> usize {
    let mut occupied: Vec<usize> = Vec::new();
    for entry in head.iter().chain(rows).chain(foot) {
        let TableEntry::Row(row) = entry else { continue; };
        let mut column = 0;
        for cell in &row.cells {
            let span = |key| cell.attrs.pairs.iter().find(|(k, _)| k == key).and_then(|(_, value)| value.parse::<usize>().ok()).unwrap_or(1).max(1);
            let width = span("colspan");
            while (column..column + width).any(|i| occupied.get(i).is_some_and(|&n| n > 0)) { column += 1; }
            occupied.resize(occupied.len().max(column + width), 0);
            occupied[column..column + width].fill(span("rowspan"));
            column += width;
        }
        for count in &mut occupied { *count = count.saturating_sub(1); }
    }
    occupied.len().max(aligns.len()).max(1)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Panel(Panel),
    Element { tag: String, attrs: Attr, children: Vec<Block> },
    Operation(Operation),
    Paragraph { attrs: Attr, children: Vec<Inline> },
    Heading { level: u8, attrs: Attr, children: Vec<Inline>, number: Option<AssignedNumber> },
    BlockQuote { attrs: Attr, children: Vec<Block> },
    List { attrs: Attr, ordered: bool, start: usize, tight: bool, items: Vec<ListItem> },
    DefinitionList { attrs: Attr, items: Vec<DefinitionItem> },
    CodeBlock { attrs: Attr, lang: Option<String>, text: String },
    Html { raw: String },
    ThematicBreak { attrs: Attr },
    Table {
        attrs: Attr, aligns: Vec<Align>, head: Vec<TableEntry>, rows: Vec<TableEntry>, foot: Vec<TableEntry>, caption: Vec<Inline>,
        number: Option<AssignedNumber>
    },
    Div { attrs: Attr, children: Vec<Block> },
    Math { attrs: Attr, tex: String, number: Option<AssignedNumber> },
    Figure { attrs: Attr, caption: Vec<Inline>, body: Vec<Block>, number: Option<AssignedNumber> },
    Raw { format: String, text: String },
    Script { lang: String, text: String },
}

impl Block {
    pub fn prose(children: Vec<Inline>) -> Vec<Self> { vec![Self::Paragraph { attrs: Attr::default(), children }] }

    pub fn attrs(&self) -> Option<&Attr> {
        match self {
            Self::Panel(panel) => Some(&panel.attrs), Self::Paragraph { attrs, .. } | Self::Heading { attrs, .. } | Self::BlockQuote { attrs, .. }
            | Self::Element { attrs, .. } | Self::List { attrs, .. } | Self::DefinitionList { attrs, .. } | Self::CodeBlock { attrs, .. }
            | Self::ThematicBreak { attrs } | Self::Table { attrs, .. } | Self::Div { attrs, .. } | Self::Math { attrs, .. }
            | Self::Figure { attrs, .. } => Some(attrs), _ => None
        }
    }

    pub fn attrs_mut(&mut self) -> Option<&mut Attr> {
        match self {
            Self::Panel(panel) => Some(&mut panel.attrs), Self::Element { attrs, .. } => Some(attrs), Self::Paragraph { attrs, .. }
            | Self::Heading { attrs, .. } | Self::BlockQuote { attrs, .. } | Self::List { attrs, .. } | Self::DefinitionList { attrs, .. }
            | Self::CodeBlock { attrs, .. } | Self::ThematicBreak { attrs } | Self::Table { attrs, .. } | Self::Div { attrs, .. } | Self::Math { attrs, .. }
            | Self::Figure { attrs, .. } => Some(attrs), Self::Html { .. } | Self::Raw { .. } | Self::Script { .. } | Self::Operation(_) => None
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Reference { attrs: Attr, items: Vec<Reference> }, Element { tag: String, attrs: Attr, children: Vec<Inline> }, Text(String), SoftBreak, HardBreak,
    Emph { attrs: Attr, children: Vec<Inline> }, Strong { attrs: Attr, children: Vec<Inline> }, Strike { attrs: Attr, children: Vec<Inline> },
    Superscript { attrs: Attr, children: Vec<Inline> }, Subscript { attrs: Attr, children: Vec<Inline> }, Highlight { attrs: Attr, children: Vec<Inline> },
    Code { attrs: Attr, text: String }, Link { attrs: Attr, children: Vec<Inline>, url: String, title: Option<String> },
    Image { attrs: Attr, alt: String, url: String, title: Option<String> }, Html(String), Operation(Operation),
    Math { attrs: Attr, display: bool, tex: String }, FootnoteRef { label: String }, Span { attrs: Attr, children: Vec<Inline> },
    Raw { format: String, text: String }
}

impl Inline {
    pub fn attrs(&self) -> Option<&Attr> {
        match self {
            Self::Emph { attrs, .. } | Self::Strong { attrs, .. } | Self::Strike { attrs, .. } | Self::Superscript { attrs, .. } | Self::Subscript { attrs, .. }
            | Self::Highlight { attrs, .. } | Self::Code { attrs, .. } | Self::Link { attrs, .. } | Self::Image { attrs, .. } | Self::Math { attrs, .. }
            | Self::Span { attrs, .. } | Self::Reference { attrs, .. } | Self::Element { attrs, .. } => Some(attrs), _ => None
        }
    }
    pub fn attrs_mut(&mut self) -> Option<&mut Attr> {
        match self {
            Self::Emph { attrs, .. } | Self::Strong { attrs, .. } | Self::Strike { attrs, .. } | Self::Superscript { attrs, .. } | Self::Subscript { attrs, .. }
            | Self::Highlight { attrs, .. } | Self::Code { attrs, .. } | Self::Link { attrs, .. } | Self::Image { attrs, .. } | Self::Math { attrs, .. }
            | Self::Span { attrs, .. } | Self::Reference { attrs, .. } | Self::Element { attrs, .. } => Some(attrs), _ => None
        }
    }
}
