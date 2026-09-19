# ADR 0002: Use funded LMSR and exact ledger units

Status: **accepted and implemented**

## Context

Every category needs an immediately available price even when no matching participant exists. Binary and small categorical questions should share one mechanism, and simulated balances must remain auditable under retries, concurrency, and settlement.

## Decision

Use a funded logarithmic market scoring rule (LMSR) with 2–8 mutually exclusive outcomes, zero initial inventory, and uniform opening prices.

For inventory vector `q`, fixed liquidity `b`, and outcome `i`:

```text
C(q) = b * ln(sum(exp(q_i / b)))
price_i = exp(q_i / b) / sum(exp(q_j / b))
trade amount = C(q after) - C(q before)
```

The instance reserve starts with at least `b * ln(n)` units, rounded up with one extra microcredit at the calculation boundary. `b` cannot change after publication.

## Units and rounding

- One unit is 1,000,000 microcredits.
- One share is 1,000 millishares.
- Balances, quantities, inventory, and ledger amounts use bounded integers.
- Financial values cross JSON as integer strings when JavaScript precision could be unsafe.
- A full winning share credits exactly one unit.
- Participant buys round up to a microcredit; sales round down.
- There is no margin, leverage, or short selling. Trading fees were later added by [ADR 0004](0004-trade-fees.md); they are charged outside this engine.

## Numerical policy

`rust_decimal` is authoritative for production arithmetic; display probabilities alone convert to `f64`. Exponentials request tolerance `1e-25`. Inventory weights are shifted by their maximum, and the trade calculation uses a stable cost-difference expression instead of subtracting two large cost totals.

Logarithms and exponentials remain approximations. The independent fixture generator uses Python `Decimal` at 80-digit precision. All 384 retained inputs must produce the same rounded microcredit amount in Rust.

Supported bounds are:

| Input | Bound |
|---|---|
| Outcome count | 2–8 |
| Liquidity `b` | 10–100,000 units |
| Quantity per request | 1–100,000 millishares (0.001–100 shares) |
| Inventory per outcome | At most 1,000,000,000 millishares |
| Inventory spread | At most `20 * b` shares |

Out-of-range trades are rejected rather than silently approximated.

## Quote and execution policy

The quote calculation is deterministic for a given inventory, liquidity, outcome, side, and quantity. The signed quote also contains time-dependent expiry information. It binds the account, instance, outcome, side, quantity, rounded amount, inventory version, expiry, and engine version.

Quotes expire after 15 seconds or at market close, whichever comes first, and reserve no inventory. Execution recomputes the result and rejects an expired quote, changed market version, engine mismatch, insufficient balance/holdings, or violated cost/proceeds limit. A successful idempotency key retrieves the same receipt even after the quote later expires.

## Accounting policy

Every balance movement is one `ledger_transfers` row with distinct source and destination accounts. A trigger applies both balance changes in the same transaction. The issuance account is the only account allowed to be negative. Grants and market subsidies draw from the finite treasury; buys and sells exchange units with one reserve; resolution credits draw from that reserve; unused reserve returns to the treasury.

The reserve must cover the largest possible unresolved outcome liability after every trade. Reconciliation independently compares balances with ledger entries, inventory with positions, and reserve with remaining claims.

## Consequences and limits

Larger trades change their own average execution price, so quantity multiplied by the displayed marginal probability is not the trade amount. One distinct trade can consume a given market version. High concentration is intentionally bounded. The fixture set is strong numerical evidence over sampled and boundary inputs, not a formal proof of all transcendental rounding cases.

## Evidence

- `backend/src/amm.rs`
- `backend/src/execution.rs`
- `backend/migrations/0001_outcome_markets.sql`
- `backend/tests/numerical.rs`
- `backend/tests/fixtures/lmsr-reference.json`
- [Trading and accounting guide](../developer/trading-and-accounting.md)

