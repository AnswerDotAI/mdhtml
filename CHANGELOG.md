# Release notes

<!-- do not remove -->

## 0.1.17

### New Features

- `viewmd` renders notebooks via aidialog's new `dlg2md`/`msg2md` (built on fastcore's new `render_md`): standard notebook output ordering (streams merged, results last), progress-bar carriage returns cleaned up, and jpeg/svg/latex/javascript outputs now supported
- Lower a table `width` attribute to an inline style width (bare number = px, invalid left visible, beats `colwidths`); `viewmd` adopts typrose as its typography layer, with the light/dark toggle flipping `prose-invert`
- Add the `details` collapsible block: `::: {.details}` lowers to `<details>` in HTML with a first-child heading as `<summary>`, degrades to a bold label elsewhere; class word reserved in DIALECT.md's new converter-obligations section
- Move `auto_ids` from `to_mdhtml` to `to_html` (on by default there) and drop the `data-auto-id` marker: derived heading ids are an export concern; merge docs/MD.md into docs/DIALECT.md as the single dialect spec
- Add `markdown="1"` containers: python-markdown-style Markdown inside subset raw HTML (per-element, non-inheriting; table cells via `<td markdown="1">`), replacing the short-lived `<md>` tag
- Render tool-call wire blocks (fenced `json {.tool}`/`{.usage}`) as folded details in `viewmd` dialog replies, via aidialog's `fmt_tools`
- Add `replacements` text-callback combinator and `DASHES` Pandoc-style dash/ellipsis pairs; render solveit dialogs in `viewmd`
- Simplify md dialect: drop setext/lazy-continuation/smart/abbrev/grid-tables/markdown="1"/tagfilter, enforce a closed raw-HTML subset, and add md self-highlighting plus notebook rendering ([#29](https://github.com/AnswerDotAI/mdhtml/issues/29))
- Add frontmatter metadata, mermaid diagram support, and raw-text/HTML sanitization to mdhtml ([#28](https://github.com/AnswerDotAI/mdhtml/issues/28))

