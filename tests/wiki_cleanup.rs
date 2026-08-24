use fast5ever::{DOCUMENT, parse_fragment};
use mdhtml::wiki_cleanup::{TemplateInfo, TemplateLookup, clean};

const LONG: &str = "This paragraph contains enough ordinary article text to pass the historical sixty character body filter.";

fn op(name: &str, args: &[&str]) -> String {
    format!("<template data-op=\"mediawiki:transclude\" data-name=\"{name}\">{}</template>", args.iter().map(|arg| format!("<div data-arg=\"\">{arg}</div>")).collect::<String>())
}

fn named_op(name: &str, positional: &[&str], named: &[(&str, &str)]) -> String {
    let positional = positional.iter().map(|arg| format!("<div data-arg=\"\">{arg}</div>")).collect::<String>();
    let named = named.iter().map(|(name, arg)| format!("<div data-arg=\"\" data-name=\"{name}\">{arg}</div>")).collect::<String>();
    format!("<template data-op=\"mediawiki:transclude\" data-name=\"{name}\">{positional}{named}</template>")
}

fn cleaned(source: &str, lookup: &dyn TemplateLookup) -> String {
    let mut dom = parse_fragment(source, "body");
    assert!(clean(&mut dom, lookup));
    dom.to_html(DOCUMENT)
}

#[test]
fn content_and_section_rules() {
    let source = format!(
        "<p>{LONG} <a href=\"./Page\"><em>linked</em></a><span class=\"citation\">citation</span><img src=\"x\">{}{}</p><h2>References</h2><p>remove</p><h2>History</h2><p>keep</p>",
        op("sfn", &["note"]),
        op("unknown", &["kept"])
    );
    let html = cleaned(&source, &());
    assert!(html.contains("<em>linked</em>") && !html.contains("<a "));
    assert!(!html.contains("citation") && !html.contains("<img") && !html.contains("data-name=\"sfn\""));
    assert!(html.contains("data-name=\"unknown\"") && !html.contains("remove") && html.contains("keep"));
}

#[test]
fn value_and_presentational_templates() {
    let source = format!(
        "<p>{LONG} {} {} {} {} {} {} {}</p>",
        named_op("val", &["7"], &[("e", "-29"), ("u", "g/cm3")]),
        named_op("val", &["4.7", "0.20"], &[("u", "μs")]),
        named_op("val", &["1.65", "+0.4", "-0.1"], &[("ul", "μs")]),
        op("frac", &["26", "1", "2"]),
        named_op("chem2", &["H3O+"], &[("link", "hydronium")]),
        op("lang", &["fr", "raison"]),
        op("nowrap", &["0.032 <a href=\"https://example.com\"><em>Pa</em></a>"])
    );
    let html = cleaned(&source, &());
    assert!(html.contains("7 \\times 10^{-29}</span> g/cm3"));
    assert!(html.contains("4.7 ± 0.20 μs") && html.contains("1.65^{+0.4}_{-0.1}</span> μs"));
    assert!(html.contains("26 1/2") && html.contains("H3O+") && html.contains("<em>raison</em>"));
    assert!(html.contains("0.032 <a href=\"https://example.com\"><em>Pa</em></a>") && !html.contains("data-name=\"nowrap\""));
}

struct Lookup;

impl TemplateLookup for Lookup {
    fn template(&self, name: &str) -> TemplateInfo {
        match name {
            "ndash" => TemplateInfo { name: "En_dash".into(), drop: false },
            "main" => TemplateInfo { name: "Main".into(), drop: true },
            _ => TemplateInfo { name: name.into(), drop: false },
        }
    }
}

#[test]
fn metadata_roles_and_anchors() {
    let source = format!("<p>{LONG} {}{}{}{}</p>", op("main", &["Physics"]), op("flag", &["Algeria"]), op("anchor", &["one place", "two"]), op("ndash", &[]));
    let html = cleaned(&source, &Lookup);
    assert!(!html.contains("data-name=\"main\"") && html.contains("data-name=\"flag\""));
    assert!(html.contains("<span id=\"one_place\"></span><span id=\"two\"></span>–"));
}

#[test]
fn raw_and_short_pages() {
    let source = format!(
        "<p>{LONG}</p><script type=\"application/vnd.mdhtml.raw\" data-format=\"wikitext\">block raw</script><p>keep <script type=\"application/vnd.mdhtml.raw\" data-format=\"wikitext\">{{{{unknown}}}}</script></p><p><script type=\"application/vnd.mdhtml.raw\" data-format=\"html\">&lt;b&gt;x&lt;/b&gt;</script></p>"
    );
    let html = cleaned(&source, &());
    assert!(!html.contains("block raw") && !html.contains("data-format=\"html\"") && html.contains("{{unknown}}"));
    let mut short = parse_fragment("<p>short</p>", "body");
    assert!(!clean(&mut short, &()));
}
