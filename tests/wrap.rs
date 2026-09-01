use mdhtml::wrap_md;

#[test]
fn unwraps_only_paragraphs() {
    let source = "---\ntitle: A long title\n---\n\n# A long heading stays put\n\nA prose paragraph\nthat was wrapped.\n{: .lead}\n\n```python\nx = 'code block'\n```\n\n| A | B |\n|---|---|\n| a | b |\n";
    let expected = "---\ntitle: A long title\n---\n\n# A long heading stays put\n\nA prose paragraph that was wrapped.\n{: .lead}\n\n```python\nx = 'code block'\n```\n\n| A | B |\n|---|---|\n| a | b |\n";
    assert_eq!(wrap_md(source, None), expected);
}

#[test]
fn wraps_nested_paragraphs_and_preserves_hard_breaks() {
    let source = "> - Alpha beta gamma delta epsilon\n\n- [ ] Alpha beta gamma delta\n\n[^n]: Alpha beta gamma delta\n\nAlpha beta\\\ngamma delta epsilon\n";
    let expected = "> - Alpha beta\n>   gamma delta\n>   epsilon\n\n- [ ] Alpha beta\n  gamma delta\n\n[^n]: Alpha beta\n    gamma delta\n\nAlpha beta\\\ngamma delta\nepsilon\n";
    assert_eq!(wrap_md(source, Some(16)), expected);
}

#[test]
fn inline_atoms_are_not_split() {
    let source = "Use `hello world` and [the docs](https://example.com/a \"long title\") here.\n";
    let wrapped = wrap_md(source, Some(18));
    assert!(wrapped.contains("`hello world`"));
    assert!(wrapped.contains("(https://example.com/a \"long title\")"));
    assert_eq!(wrap_md(&wrapped, Some(18)), wrapped);
}

#[test]
fn wrapping_does_not_create_blocks() {
    let source = "alpha beta - item gamma delta # heading epsilon zeta {: .x}\n";
    let wrapped = wrap_md(source, Some(10));
    assert!(!wrapped.contains("\n- "));
    assert!(!wrapped.contains("\n# "));
    assert!(!wrapped.contains("\n{: "));
}

#[test]
fn preserves_crlf_and_final_newline_state() {
    assert_eq!(wrap_md("One two\r\nthree four\r\n", None), "One two three four\r\n");
    assert_eq!(wrap_md("One two\nthree four", None), "One two three four");
}

#[test]
fn preserves_nonbreaking_spaces() {
    let source = "e.g.\u{a0}this stays together\n\n\u{a0}edge\u{a0}\n";
    assert_eq!(wrap_md(source, None), source);
    assert!(wrap_md(source, Some(8)).contains("e.g.\u{a0}this"));
}

#[test]
fn preserves_raw_html_regions() {
    let source = "<style>\n.x { color: red }\n</style>\n\n<details>\n\n<summary>\n\nThinking\n</summary>\n\nMore details\n</details>\n";
    assert_eq!(wrap_md(source, None), source);
    assert_eq!(wrap_md("<https://example.com>\ncontinues\n", None), "<https://example.com> continues\n");
}
