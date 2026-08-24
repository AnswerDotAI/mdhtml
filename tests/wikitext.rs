use mdhtml::{wiki2md, wiki2mdhtml};

#[test]
fn lowers_core_wikitext_structure() {
    let source = r#"== Life ==

'''Alan Turing''' was an [[English people|English]] mathematician.<ref name="bio">citation</ref>

* one
* two with [https://example.com a source]
"#;
    let html = wiki2mdhtml(source);
    assert!(html.contains("<h2>Life</h2>"), "{html}");
    assert!(html.contains("<strong>Alan Turing</strong>"), "{html}");
    assert!(html.contains("<a href=\"./English_people\">English</a>"), "{html}");
    assert!(html.contains("<a href=\"https://example.com\">a source</a>"), "{html}");
    assert!(!html.contains("citation"), "{html}");
    assert!(html.contains("<ul>"), "{html}");
}

#[test]
fn templates_are_semantic_instructions() {
    let html = wiki2mdhtml("Distance: {{convert|12|km|abbr=on}}; {{{label|unknown}}}; {{#if:x|yes|no}}.");
    assert!(html.contains("<template data-op=\"mediawiki:transclude\" data-name=\"convert\">"), "{html}");
    assert!(html.contains("<div data-arg>12</div><div data-arg>km</div><div data-arg data-name=\"abbr\">on</div>"), "{html}");
    assert!(html.contains("<template data-op=\"mediawiki:parameter\" data-name=\"label\">"), "{html}");
    assert!(html.contains("<template data-op=\"mediawiki:function\" data-name=\"if\">"), "{html}");
}

#[test]
fn simple_tables_lower_and_structural_expansion_falls_back() {
    let source = r#"{| class="wikitable"
! Name
! Value
|-
| a
| <math>x-y</math>
|-
| b
| 2
|}"#;
    let markdown = wiki2md(source);
    assert!(markdown.contains("| Name | Value |"), "{markdown}");
    assert!(markdown.contains(r"| a | \(x-y\) |"), "{markdown}");
    let html = wiki2mdhtml(source);
    assert!(html.contains("<table class=\"wikitable\">"), "{html}");
    assert!(html.contains("<th>Name</th><th>Value</th>"), "{html}");
    assert!(html.contains("<td>a</td><td><span class=\"math inline\">x-y</span></td>"), "{html}");

    let raw = wiki2mdhtml("{|\n! a {{!}} b\n|}");
    assert!(raw.contains("data-format=\"wikitext\""), "{raw}");
}

#[test]
fn unsupported_media_stays_inert_raw_wikitext() {
    let markdown = wiki2md("Before [[File:Plot.png|thumb|Plot]] after");
    assert!(markdown.contains("{=wikitext}"), "{markdown}");
    let html = wiki2mdhtml("Before [[File:Plot.png|thumb|Plot]] after");
    assert!(html.contains("<script type=\"application/vnd.mdhtml.raw\" data-format=\"wikitext\">"), "{html}");
}

#[test]
fn math_uses_bracket_delimiters() {
    let markdown = wiki2md("Inline <math>x-y</math>.\n\n<math display=\"block\">z^2</math>\n\n:<math>q</math>.");
    assert!(markdown.contains(r"Inline \(x-y\)."), "{markdown}");
    assert!(markdown.contains("\\[\nz^2\n\\]"), "{markdown}");
    assert!(markdown.contains("\\[\nq.\n\\]"), "{markdown}");
    let html = wiki2mdhtml("Inline <math>x-y</math>.\n\n<math display=\"block\">z^2</math>");
    assert!(html.contains("<span class=\"math inline\">x-y</span>"), "{html}");
    assert!(html.contains("<div class=\"math display\">z^2</div>"), "{html}");
}
