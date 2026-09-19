# ADR 0004: Charge a per-trade fee split with the market creator

Status: **accepted and implemented**

## Context

[ADR 0002](0002-lmsr-accounting.md) prices every trade with a pure funded LMSR. Because the cost function is path-independent, a buy/sell round trip returns everything except at most one microcredit of rounding, so the platform earned no per-trade revenue: its only income was settlement surplus, which exists only when participants net-buys lose. On an efficient or informed market the treasury expects to bleed subsidy.

The platform also has no market-creator role: instances are created by the administrator, and 100% of any settlement surplus returns to the treasury. A creator economy was requested so that whoever publishes a market earns from the activity it attracts.

## Decision

Charge a trading fee of 25 basis points of the LMSR amount on every trade, and split the accumulated fee pot 50/50 between the platform treasury and the instance's recorded creator at settlement.

- The fee is policy, not engine math: `fee::trade_fee` lives outside `amm.rs`, so the pure LMSR numerical contract, fixtures, and bounds are unchanged.
- `fee = ceil(amount × 25 / 10,000)`, rounded against the trader on both sides, so a fee can never exceed its amount and a sale never credits below zero.
- Quotes and receipts expose all-in amounts: `amount_micros` is the debit/credit including the fee, with `fee_micros` reported separately. `limit_micros` bounds the all-in amount.
- The fee rides inside the single trader/reserve ledger transfer. This adds no hot row: per-trade transfers to `treasury` would serialize all trades on the treasury account row, while the reserve is already locked per instance.
- `trades.fee_micros` records each fee; `instances.creator_account_id` records the optional creator (validated to be a participant account, immutable after publication).
- At settlement, after the last claim, the pot `sum(fee_micros)` leaves the reserve as explicit `fee` transfers: the creator's floor half (`fee:{id}:creator`) and the treasury's remainder including any odd microcredit (`fee:{id}:treasury`). Without a creator, the whole pot is treasury revenue. The remaining reserve balance still releases to the treasury as before.

The fee only ever adds to the reserve relative to the fee-free accounting, so ADR 0002's reserve-coverage invariant and reconciliation checks remain valid unchanged.

## Alternatives considered

- **No fee (status quo)**: keeps trading cheapest, but platform income depends on the crowd being wrong and the treasury bleeds subsidy on efficient markets.
- **Per-trade transfers straight to treasury/creator**: simpler mental model, but every trade would lock and update the treasury account row, serializing all trades and collapsing cross-market throughput.
- **Surplus split at settlement instead of fee split**: pays creators only when participants net-lose on the market, which rewards markets that attract mistaken traders and has void/no-trade edge cases. The fee split pays for activity, not for crowd error.

## Consequences and limits

- Round trips now cost two fees instead of ≤1 microcredit; the effective spread is 25 bps per side on the LMSR amount.
- The fee rate and the 50/50 split are constants in `fee.rs`, not operator configuration; changing either is a deliberate contract change requiring this record to be superseded.
- Creator attribution requires an account at instance creation; there is no transfer or reattribution path, matching the immutability of every other published field.
- Platform-created (administrator) instances have no creator, so their full fee pot is treasury revenue; a future user-created-market feature can populate the column without further schema change.

## Evidence

- `backend/src/fee.rs`
- `backend/src/execution.rs` (all-in quotes, fee verification, all-in transfer)
- `backend/src/resolution.rs` (settlement fee split)
- `backend/migrations/0005_trade_fees.sql`
- `backend/tests/integration.rs` (`trading_fees_are_charged_and_split_with_the_creator`)
- [Trading and accounting guide](../developer/trading-and-accounting.md)
