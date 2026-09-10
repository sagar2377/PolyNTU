# Trading and accounting internals

This document follows a trade from share input through LMSR calculation, signed quote, database execution, ledger movement, receipt, and reconciliation.

## Fixed units

| Quantity | Internal representation | JSON representation |
|---|---|---|
| 1 simulated unit | 1,000,000 microcredits | decimal integer string for balances/amounts |
| 1 outcome share | 1,000 millishares | JSON integer |
| 0.001 share | 1 millishare | minimum request quantity |

The conversion is chosen so one winning millishare redeems for 1,000 microcredits and one winning full share redeems for exactly 1,000,000 microcredits.

The frontend formats microcredit strings with `BigInt`. It accepts share input with at most three decimal places and converts it to a safe JavaScript integer from 1 to 100,000 millishares.

## LMSR state

Each instance owns:

- a fixed ordered outcome list;
- an inventory vector `q` in millishares;
- a fixed liquidity parameter `b` in units;
- a monotonically increasing instance version; and
- one funded reserve account.

For conceptual share quantities in full shares:

```text
C(q) = b * ln(sum(exp(q_i / b)))
p_i  = exp(q_i / b) / sum(exp(q_j / b))
```

The marginal prices sum to one. A finite trade pays the difference in cost, not quantity times the initial marginal price.

## Initial funding

At zero inventory the worst-case LMSR loss is bounded by `b * ln(n)` units for `n` outcomes. `amm::funding` calculates:

```text
ceil(b * 1,000,000 * ln(n)) + 1 microcredit
```

The extra microcredit protects the boundary after decimal approximation. Instance creation transfers this amount from treasury to a dedicated reserve before inserting the instance.

## Stable calculation

`amm::calculate`:

1. validates current inventory and liquidity;
2. checks outcome and request quantity;
3. applies positive delta for a buy or negative delta for a sale;
4. validates new inventory and concentration;
5. computes current decimal probabilities after subtracting the maximum inventory weight;
6. computes `growth = exp(delta / (b * SHARE_SCALE))`;
7. computes the cost argument `1 + p_i * (growth - 1)`;
8. calculates `b * CREDIT_SCALE * ln(argument)`;
9. rounds a buy debit up or a sale credit down; and
10. returns new inventory and display probabilities.

This formula avoids subtracting two large approximate cost totals. The supported concentration spread is at most `20*b` shares, limiting exponential extremes.

## Worked binary quote

At uniform binary inventory `[0,0]`, liquidity `b=100`, and a buy of 10 shares (`10,000` millishares) in outcome 0, the retained reference result is:

```text
amount_micros = 5,124,948
average price = 5.124948 units / 10 shares = 0.5124948
```

The marginal price begins at 0.5 and ends above 0.5. Selling the same 10 shares immediately returns no more than the debit; the tested rounding difference is at most one microcredit.

## Quote contract

A quote reads current instance state and creates signed `QuoteClaims` containing:

- authenticated account ID;
- instance and outcome IDs;
- side and quantity;
- exact rounded amount;
- instance version;
- expiry at `min(now + 15 seconds, close)`; and
- engine compatibility version.

The JSON body is base64url-encoded and authenticated with HMAC-SHA256. Clients must treat the token as opaque. A quote changes no database state and does not guarantee later execution.

## User limits

`limit_micros` is always a nonnegative integer string:

- buy: reject when recalculated debit is greater than the limit;
- sell: reject when recalculated proceeds are less than the limit.

The current UI uses the quoted amount, confirming exactly the displayed financial result. The field leaves room for future clients to express a compatible bound without trusting the displayed floating-point probability.

## Idempotency

The client supplies one 1–120 character key per intended trade. The server hashes the serialized `TradeRequest`, which includes quote token and limit.

| Existing row | Result |
|---|---|
| None | Insert an in-progress claim and continue. |
| Same request hash, response null | Wait for/serialize with the row lock, then continue if the other transaction rolled back. |
| Same request hash, response present | Return the stored receipt without rechecking expiry or state. |
| Different request hash | Return 409 conflict. |

The idempotency insertion, execution, response update, and all trade effects share one transaction. A rollback removes the in-progress insert and permits the exact retry to attempt execution again.

## Execution locking

The service locks:

```text
idempotency row (FOR UPDATE)
    -> instance row (FOR UPDATE)
        -> participant and reserve accounts, sorted (FOR NO KEY UPDATE)
```

It then reads the position without an additional position lock. The instance lock serializes every trade that could update this instance's inventory or positions. The participant account lock serializes spending across different instances.

This design avoids a discovered lock-upgrade cycle: foreign-key checks may hold `KEY SHARE` on the user account before balance locking; `FOR NO KEY UPDATE` is compatible with that weaker lock.

## Checks performed under locks

After waiting for locks, execution rechecks:

1. database time is before both quote expiry and market close;
2. persisted state is open and not suspended;
3. signed account and engine are correct;
4. signed version equals current instance version;
5. recomputed rounded amount equals the signed amount;
6. user limit accepts the amount;
7. buyer balance covers the debit or seller holdings cover the quantity; and
8. resulting reserve balance covers maximum inventory liability.

Checking time after locking prevents a request queued before close from executing after close.

## Atomic writes

A successful trade transaction writes:

1. a ledger transfer (`buy` user→reserve or `sell` reserve→user);
2. updated materialized balances through the transfer trigger;
3. the account/outcome position;
4. new instance inventory and exactly one version increment;
5. one immutable trade using that resulting version;
6. the durable idempotency receipt; and
7. one `trade` outbox event.

The commit occurs before the HTTP response. A failure at any insertion rolls all seven effects back.

## Receipt fields

The receipt identifies the trade and instance/outcome, repeats side/quantity/amount, and returns post-trade balance, post-trade owned quantity, resulting instance version, and server creation time. It is stored as JSON in the idempotency row, so future exact retries return byte-equivalent data after serialization.

## Ledger flows

```text
issuance --initial issuance--> treasury
treasury --grant-------------> user
treasury --subsidy-----------> reserve
user     --buy---------------> reserve
reserve  --sell--------------> user
reserve  --resolution--------> user
reserve  --release-----------> treasury
```

The issuance account is deliberately negative by the total issued amount. All other account kinds have database nonnegative checks. Summing every account balance should remain zero.

## Settlement accounting

For a winner, each winning millishare credits 1,000 microcredits. For a void across `n` outcomes, the account's positive millishares across all outcomes are summed, multiplied by 1,000, divided by `n`, and rounded down by integer division.

Historical positions remain; a unique settlement claim records the actual aggregate credit. This prevents a multi-outcome account credit from appearing once per position.

## Reconciliation

`GET /admin/reconcile` checks four independent facts in one repeatable-read snapshot:

- materialized account balances equal summed ledger entries;
- each inventory element equals summed positions for that outcome;
- each reserve covers unresolved or unclaimed obligations according to result state; and
- total balances across all accounts equal zero.

Reconciliation detects corruption but does not automatically repair it. Follow the [operations runbook](operations.md) when it fails.

## Expected conflicts versus failures

Stale versions, expired quotes, insufficient funds/shares, and price-limit failures are expected 409 outcomes and have no effects. SQL serialization/deadlock/lock-timeout errors are retried internally up to three attempts. Unknown database/internal failures return 500; the browser must preserve and retry the exact request/key because a response interruption can make commit status uncertain.

## Change checklist

Any change to scales, rounding, funding, supported bounds, quote claims, lock ordering, ledger kinds, or settlement calculation requires updates to:

- `amm.rs` and/or execution/resolution code;
- a new migration when stored constraints change;
- independent numerical fixtures rather than only unit expectations;
- concurrency, rollback, and reconciliation integration tests;
- API/frontend parsing;
- ADR 0002 and this document; and
- the traceability matrix.

