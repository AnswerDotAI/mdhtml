---
formdata:
  class_of_stock: Common Stock
  vesting_schedule: four years, with a one-year cliff
---

# 1. Offer of Employment

`{{company_common_name}}` (the "Company") is pleased to offer `{{candidate_name}}` the position of `{{job_title}}`. This letter summarizes the key terms: Sections 1.(a), 1.(b) and 1.(c).

## (a) Compensation

Your base salary will be `{{base_salary}}` per year, paid on `{{company_common_name}}`'s normal payroll schedule and subject to all withholdings required by law.[^1] Salary is reviewed annually as part of the process described in Section 1..

[^1]: Including any deductions you authorize in writing.

## (b) Equity

`{{#equity.options}}`
Subject to approval by `{{company_common_name}}`'s Board of Directors, you will be granted an option to purchase `{{shares_subject_to_option}}` shares of `{{class_of_stock}}` at a strike price equal to fair market value on the date of grant. The option will vest over `{{vesting_schedule}}`.

Your option will be granted in tranches, `{{total_shares}}` shares in all:

| Grant date | Shares |
|------------|--------|
`{{#grants}}`
| `{{date}}` | `{{shares}}` |
`{{/grants}}`

```{python}
__data__['total_shares'] = f"{sum(int(g['shares'].replace(',', '')) for g in __data__.get('grants', [])):,}"
```
`{{/equity.options}}`

`{{^equity.options}}`
This offer includes no stock option component.
`{{/equity.options}}`

`{{#equity.restricted_stock}}`
Subject to Board approval, you will be granted the right to purchase `{{number_shares}}` shares of `{{class_of_stock}}` under a Restricted Stock Purchase Agreement, vesting over `{{vesting_schedule}}`.
`{{/equity.restricted_stock}}`

Tax treatment is your responsibility; see also your cash compensation 1.(a).

## (c) At-Will Employment

Your employment with `{{company_common_name}}` is at will: either you or the Company may end it at any time, with or without cause. Nothing in Section 1.(a) or Section 1.(b) changes that.

---

To accept, sign below by `{{offer_expiration_date}}`.

<table class="sig-block" custom-style="Borderless Table">
<tr><td><strong>`{{company_common_name}}`</strong></td><td><strong>Accepted and agreed:</strong></td></tr>
<tr><td><br><br><br></td><td><br><br><br></td></tr>
<tr><td>Signature: _________________________</td><td>Signature: _________________________</td></tr>
<tr><td>`{{#manager}}`Name: `{{name}}` (`{{title}}`)`{{/manager}}`</td><td>Name: `{{candidate_name}}`</td></tr>
<tr><td>Date: `{{offer_date}}`</td><td>Date: `{{signature_date}}`</td></tr>
</table>

Please retain a copy of this letter for your records; the terms in Sections 1.(a), 1.(b) and 1.(c) are the entire agreement.
