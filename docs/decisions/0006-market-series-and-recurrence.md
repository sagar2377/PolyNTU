# ADR 0006: Market series with recurring brackets and rolling spawn

Status: **accepted and implemented**

## Context

Every instance today is a single fixed window created by the administrator, definitions are immutable once published, and there is no recurrence and no creator ownership of definitions. The bus demo template hardcodes one 10-minute window.

The product plan has external creators publishing markets, including time-natured ones like bus arrival, with configurable cadence: a bus series spawns a market every 2 minutes, at most 5 live at once per (route, direction, stop), and only during operating hours, because buses do not run at midnight.

## Decision

Markets will be published as market series: creator-owned definitions that spawn bracket instances on a recurrence rule.

- A market series is a creator-owned definition holding the options, the rule, the recurrence rule (interval, active period, maximum concurrency, optional end date), and the resolution authority ([ADR 0007](0007-resolution-authority.md)). A one-time market is a series with exactly one instance. Instances gain a series reference and a bracket slot.
- Recurrence rule fields: interval, with a 1-minute minimum; active period, a creator-set daily operating window, with brackets never spawning outside it; maximum concurrency, capped at 50; end date, optional, and its absence means perpetual: the series recurs forever with automatically refreshing resolution times.
- Rolling spawn: on every scheduler tick, while the live count is below maximum concurrency and the next slot start falls inside the active period, the scheduler will publish the bracket instance [T, T+interval), fund its reserve from the treasury, and list it. The covered horizon is maximum concurrency × interval. Example: interval 2 minutes with maximum concurrency 5 gives five live brackets covering a rolling 10-minute horizon, one new bracket every 2 minutes.
- Creator trading ban: the series creator will not be able to trade in their own instances, enforced at quote and execution time. The 50/50 fee share ([ADR 0004](0004-trade-fees.md)) is the creator's compensation; trading on superior knowledge of their own resolution would be the alternative they are giving up.
- Reserves remain treasury-funded; creators contribute definitions and earn fees, not deposits.
- The demo bus template will become a rolling series: interval 2 minutes, maximum concurrency 5.

## Alternatives considered

- **One instance with one outcome per bracket**: rejected; the engine caps direct outcomes at 2 to 8, the cadence cannot roll, and settlement would be all-or-nothing across the day.
- **Administrator-created recurring templates**: no creator economy; external parties cannot publish.
- **Creator-funded reserves**: a higher barrier with no fraud benefit while the trading ban holds; deferred.

## Consequences and limits

- A new scheduler duty and series/instance schema.
- Bracket churn multiplies instance rows, so retention and compaction need attention later.
- The series grouping is what makes the day-long probability visualization possible.
- The trading ban is one more check on the trade hot path: a comparison, not a lock.
- The void policy ([ADR 0003](0003-evidence-resolution.md)) already covers spawn gaps and missed evidence.

## Evidence

Implemented artifacts:

- `backend/migrations/0009_market_series.sql`: the `market_series` table with its recurrence, interval, active-window, concurrency, end, anchor, and state checks; the `instances` series/bracket/fee columns; the `protect_series` trigger; and the `protect_instance` extension covering the new immutable fields.
- `backend/src/market.rs`: `Schedule` (once and recurring), `NewSeries`, `Series`, the bracket timing (`bracket_spec`, `instance_spec`, `BRACKET_FINALIZE_MARGIN_MS`, `BRACKET_DEADLINE_MARGIN_MS`), `inside_active_window` in Singapore time, and `demo_series_spec`.
- `backend/src/store.rs`: `create_series` (creator-role check, templates row, immediate one-time publication), `series_view` and `series_list`, and `spawn_due_brackets`/`spawn_series_brackets` (rolling spawn and series ending).
- `backend/src/api.rs`: `POST /api/v2/series`, `GET /api/v2/series`, and `GET /api/v2/series/{id}`.
- `backend/src/execution.rs` and `backend/src/fee.rs`: the per-market fee attribute and the creator trading ban at quote and execution.
- `backend/src/worker.rs`: bracket spawning inside `tick` and the demo bus series seeding in `seed_demo`.
- `frontend/src/pages/SeriesPage.jsx` and `frontend/src/pages/CreateMarket.jsx`, plus the browse series strip, the market page series link and fee display, and the creator navigation in `App.jsx`.
- `backend/tests/integration.rs`: one-time publication with member rejection, the rolling horizon grid, end-date stop with voiding and series end, the fee-free market with no creator payout, the creator trading ban, and the rolling demo bus.

Honest notes extending this record's original wording:

- The per-market fee policy extends this record and amends [ADR 0004](0004-trade-fees.md)'s blanket fee. `fee_charged` is a market attribute fixed at creation, defaulting to true: the administrator sets it when creating instances through the admin route, and creators set it when publishing a series. A fee-free market charges nothing, collects no fee, and pays no creator share. The demo bus series is fee-free: its purpose is student welfare, crowd-sourcing real-time road traffic and arrival estimation. `fee::charged` gained the `fee_charged` parameter.
- `max_concurrency` bounds the upcoming tradable horizon, not the total live count. The scheduler keeps up to `max_concurrency` upcoming grid slots published; brackets already in observation or settlement can briefly coexist with the full horizon, so the total non-terminal count can exceed `max_concurrency`.
- Scheduler-spawned brackets are not administrator audit rows; only the outbox `opened` event is written, while direct creations still audit. Each series also gets a `templates` row with the series ID, so browse grouping keeps working.
