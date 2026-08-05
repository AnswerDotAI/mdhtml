#let mdhtml-numbering(..ns) = {
  let n = ns.pos()
  if n.len() == 1 { numbering("1", n.at(0)) + "." }
  else if n.len() == 2 { numbering("1", n.at(0)) + ".(" + numbering("a", n.at(1)) + ")" }
  else if n.len() == 3 { numbering("1", n.at(0)) + ".(" + numbering("a", n.at(1)) + ")(" + numbering("i", n.at(2)) + ")" }
  else if n.len() == 4 { numbering("1", n.at(0)) + ".(" + numbering("a", n.at(1)) + ")(" + numbering("i", n.at(2)) + ")(" + numbering("A", n.at(3)) + ")" }
  else if n.len() == 5 { numbering("1", n.at(0)) + ".(" + numbering("a", n.at(1)) + ")(" + numbering("i", n.at(2)) + ")(" + numbering("A", n.at(3)) + ")(" + numbering("I", n.at(4)) + ")" }
  else if n.len() == 6 { numbering("1", n.at(0)) + ".(" + numbering("a", n.at(1)) + ")(" + numbering("i", n.at(2)) + ")(" + numbering("A", n.at(3)) + ")(" + numbering("I", n.at(4)) + ")(" + numbering("1", n.at(5)) + ")" }
}
#set heading(numbering: mdhtml-numbering)

= Offer of Employment <sec-offer>

#raw("{{company_common_name}}") (the "Company") is pleased to offer #raw("{{candidate_name}}") the position of #raw("{{job_title}}"). This letter summarizes the key terms: #ref(<sec-comp>, supplement: [Sections]), #ref(<sec-equity>, supplement: none) and #ref(<sec-atwill>, supplement: none).

== Compensation <sec-comp>

Your base salary will be #raw("{{base_salary}}") per year, paid on #raw("{{company_common_name}}")'s normal payroll schedule and subject to all withholdings required by law.#footnote[Including any deductions you authorize in writing.] <fn-1> Salary is reviewed annually as part of the process described in #ref(<sec-offer>, supplement: [Section]).

== Equity <sec-equity>

#raw("{{#equity.options}}")

Subject to approval by #raw("{{company_common_name}}")'s Board of Directors, you will be granted an option to purchase #raw("{{shares_subject_to_option}}") shares of #raw("{{class_of_stock}}") at a strike price equal to fair market value on the date of grant. The option will vest over #raw("{{vesting_schedule}}").

#raw("{{/equity.options}}")

#raw("{{#equity.restricted_stock}}")

Subject to Board approval, you will be granted the right to purchase #raw("{{number_shares}}") shares of #raw("{{class_of_stock}}") under a Restricted Stock Purchase Agreement, vesting over #raw("{{vesting_schedule}}").

#raw("{{/equity.restricted_stock}}")

Your grants under this offer are:

#table(
  columns: 4,
  table.header([Grant date], [Shares], [Class], [Vesting]),
  [#raw("{{#grants}}")], [], [], [],
  [#raw("{{grant_date}}")], [#raw("{{shares}}")], [#raw("{{class_of_stock}}")], [#raw("{{vesting_schedule}}")],
  [#raw("{{/grants}}")], [], [], [],
)

Tax treatment is your responsibility; see also #ref(<sec-comp>, supplement: [your cash compensation]).

== Contingencies <sec-conting>

This offer is contingent on each of the following:

#raw("{{#contingencies}}")

- #raw("{{.}}")

#raw("{{/contingencies}}")

== At-Will Employment <sec-atwill>

Your employment with #raw("{{company_common_name}}") is at will: either you or the Company may end it at any time, with or without cause. Nothing in #ref(<sec-comp>, supplement: [Section]) or #ref(<sec-equity>, supplement: [Section]) changes that.

#line(length: 100%)

To accept, sign below by #raw("{{offer_expiration_date}}").

#table(
  columns: 2,
  stroke: none,
  [#strong[#raw("{{company_common_name}}")]], [#strong[Accepted and agreed:]],
  [\ \ \ ], [\ \ \ ],
  [Signature: \_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_], [Signature: \_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_],
  [Name: #raw("{{hiring_manager_name}}")], [Name: #raw("{{candidate_name}}")],
  [Date: #raw("{{offer_date}}")], [Date: #raw("{{signature_date}}")],
)

Please retain a copy of this letter for your records; the terms in #ref(<sec-comp>, supplement: [Sections]), #ref(<sec-equity>, supplement: none) and #ref(<sec-atwill>, supplement: none) are the entire agreement.
