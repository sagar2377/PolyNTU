# PolyNTU glossary

## Product and market terms

| Term | Definition |
|---|---|
| Account | One participant or platform balance record. User accounts are accessed with bearer tokens. |
| Automated market maker (AMM) | Algorithm that quotes and accepts trades from inventory without matching another participant. |
| Category | Rule/evidence family: weather, bus, elections, queue/crowd, or attendance. |
| Evidence | Normalized post-window observation used to propose an instance result. |
| Instance | One concrete market occurrence with fixed outcomes, rules, times, inventory, reserve, and result. |
| LMSR | Logarithmic market scoring rule, the common pricing mechanism. |
| Market probability | Marginal LMSR price displayed as a percentage. It reflects current inventory/trading and is not a guaranteed calibrated forecast. |
| Outcome | One of 2–8 mutually exclusive results. |
| Position | Millishares held by one account in one instance/outcome. |
| Quote | Signed, read-only, short-lived preview of a proposed trade. |
| Reserve | Per-instance funded account that receives buys, pays sales/claims, and covers possible settlement liability. |
| Resolution | Fixed winning outcome or void result after evidence/finalization rules. |
| Settlement claim | Unique record of the final aggregate credit paid to one account for one instance. |
| Suspension | Temporary flag preventing trades while the normal timing/resolution contract remains. |
| Template | Grouping/scheduling identity for recurring instances. |
| Void | Terminal policy when no winner can be selected; every share redeems at `1/n` units before account-level rounding. |

## Units and pricing

| Term | Definition |
|---|---|
| Unit | Simulated balance unit; never real currency. |
| Microcredit | One millionth of one simulated unit. |
| Share | Claim that redeems to one unit when its outcome wins. |
| Millishare | One thousandth of a share. |
| Inventory (`q`) | Outstanding millishares per outcome, equal to summed participant positions. |
| Liquidity (`b`) | Fixed LMSR parameter controlling price movement and required subsidy. Larger `b` means less movement for the same trade. |
| Marginal price | Instantaneous LMSR price before/after a trade. |
| Average execution price | Exact rounded total amount divided by trade quantity. It can differ from the initial marginal price. |
| Price concentration limit | V1 bound requiring maximum minus minimum inventory to stay within `20*b` shares. |

## Accounting

| Term | Definition |
|---|---|
| Issuance account | Sole account allowed to be negative, balancing the initial creation of simulated units. |
| Treasury | Finite platform account funding user grants and instance subsidies. |
| Ledger transfer | Append-only balanced movement from one account to another. |
| Ledger entry | Debit or credit projection of a transfer in the database view. |
| Reconciliation | Read-only comparison of balances, ledger, inventory, positions, reserves, claims, and net units. |
| Idempotency key | Client-selected identifier binding one account to one exact trade request/receipt. |
| Instance version | Integer advanced by every instance update; quotes require an exact version. |

## Evidence and time

| Term | Definition |
|---|---|
| Authoritative time | PostgreSQL wall time plus persisted demo offset, used for every cutoff decision. |
| Observation window | Fixed half-open interval `[start,end)` to which evidence applies. |
| Finalize-after time | Earliest time a complete evaluated result can be fixed. |
| Evidence deadline | Time at which missing/incomplete evidence becomes eligible for voiding; manual submissions must arrive before it. |
| Event ID | Source-provided evidence-delivery identity used for exact duplicate detection. |
| Source revision | Nonnegative source version; highest accepted revision controls finalization. |
| Completeness/finality | Explicit evidence statement that the source covered the required interval or result is final. Missing/false never automatically means No. |
| Data mode | `simulated` for worker-generated demo observations or `manual` for authenticated submitted evidence. |

## Architecture and operations

| Term | Definition |
|---|---|
| Outbox | Durable database table of committed instance events consumed by SSE clients. |
| SSE | Server-Sent Events, the one-way public update stream. |
| Effective closed state | Public view reports closed after cutoff even before worker persists the lifecycle transition. |
| Canonical documentation | Primary maintained explanation for a subject. It still requires verification against requirements and implementation. |
| Historical documentation | Material useful for understanding prior designs but not the current contract. |
| Working-tree baseline | Files currently on disk, including uncommitted changes; not uniquely identified by Git `HEAD`. |

## Terms deliberately retired from the active product

Call/put options, strike price, implied volatility, Black–Scholes, Greeks, Monte Carlo option pricing, and option contracts belong only to `legacy/python-backend/`. They must not be used to describe active outcome-share trading.
