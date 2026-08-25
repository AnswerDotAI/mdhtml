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
    assert!(html.contains("class=\"footnote-ref\"") && html.contains("citation"), "{html}");
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
fn comments_are_not_content_or_template_names() {
    let html = wiki2mdhtml("Before<!-- hidden --> after {{Multiple image
<!-- Essential parameters -->
|caption=Shown}}");
    assert!(!html.contains("hidden") && !html.contains("Essential parameters"), "{html}");
    assert!(html.contains(r#"data-name="Multiple image""#), "{html}");
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

    let fallback = wiki2mdhtml("{| style=float:right\n! {{abbr|NASA|National Aeronautics and Space Administration}}\n| {{no}}\n|}");
    assert!(fallback.starts_with("<p>{| style=float:right"), "{fallback}");
    assert!(fallback.contains("data-name=\"abbr\"") && fallback.contains("data-name=\"no\""), "{fallback}");
}

#[test]
fn media_is_a_structured_image() {
    let html = wiki2mdhtml("Before [[File:Plot.png|thumb|Plot]] after");
    assert!(html.contains("<img src=\"./File:Plot.png\" alt=\"Plot\""), "{html}");
    assert!(html.contains("data-mediawiki-source=\"[[File:Plot.png|thumb|Plot]]\""), "{html}");
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

    let multiline = wiki2mdhtml(":<math>\\rho = \\begin{cases}\n  1 & x \\le 0 \\\\\n+  2 & x > 0\n\\end{cases}</math>.");
    assert!(multiline.contains("<div class=\"math display\">\\rho = \\begin{cases}\n  1 &amp; x \\le 0"), "{multiline}");
    assert!(!multiline.contains("<pre>"), "{multiline}");
}

#[test]
fn common_literal_html_uses_document_nodes() {
    let source = "<span class=\"anchor\" id=\"point\">J<sub>e</sub><sup>2</sup></span><br><small>note</small>\n\n<blockquote>\n quoted ''text''\n</blockquote>";
    let html = wiki2mdhtml(source);
    assert!(html.contains("<span id=\"point\" class=\"anchor\">J<sub>e</sub><sup>2</sup></span><br />"), "{html}");
    assert!(html.contains("<span class=\"small\">note</span>"), "{html}");
    assert!(html.contains("<blockquote>\n<p>quoted <em>text</em></p>\n</blockquote>"), "{html}");
    assert!(!html.contains("data-format=\"html\""), "{html}");
}
