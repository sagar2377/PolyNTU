# PolyNTU general overview

## Purpose

PolyNTU is a campus-focused prediction-market demonstration. It lets participants express beliefs about future, objectively resolvable campus events by trading outcome shares with simulated units. It is an academic software project, not a real-money betting or payment system.

The current application deliberately uses direct outcomes rather than the call/put options and Black–Scholes model found in the archived prototype. Every open market has a fixed question, outcomes, observation window, evidence source, deadline, and resolution policy.

## Core concepts

| Term | Plain-language meaning |
|---|---|
| Template | A recurring or reusable market definition used to group instances. |
| Series | A published definition that occurs once or recurs in rolling brackets on a set schedule. Verified creators publish series; the platform owns the demo bus series. |
| Instance | One concrete occurrence with its own times, outcomes, inventory, reserve, evidence, and result. |
| Outcome share | A simulated claim on one mutually exclusive result. |
| Probability | The current marginal LMSR price shown as a percentage. It reflects trading, not a provider forecast. |
| Quote | A short-lived preview of the exact cost or proceeds for one proposed trade. |
| Fee | A 0.25% trading fee on fee-charging markets, fixed when the market is created and included in quotes and trades. It accumulates in the market's reserve and is split between the platform and the recorded market creator when the market settles. Welfare markets like bus timing are fee-free. |
| Position | The shares an account currently owns in an instance/outcome. |
| Evidence | A normalized observation submitted after the published observation window. |
| Settlement claim | The unique final credit recorded for one account in one instance. |
| Void | A terminal result used when the published policy cannot select a winner. Each share then redeems at `1/n` units, aggregated per account and rounded down to a microcredit. |

## Market categories

The same pricing, ledger, quote, execution, and settlement infrastructure supports every category.

| Category | Current question shape | Resolution input |
|---|---|---|
| Weather | Whether accumulated rainfall reaches a threshold | Complete rainfall total for one station and fixed window |
| Bus timings | Whether a matching bus arrives during a fixed interval | Matching actual-arrival timestamps plus complete coverage |
| Fictional elections | Which named fictional candidate wins | Sole final winner or a published no-winner/void condition |
| Queue and crowd | Whether a queue or occupancy count reaches a threshold | Complete count for the fixed metric and location |
| Event and lecture attendance | Whether unique attendance reaches a threshold | Deduplicated aggregate attendance count |

## User journey

1. A participant registers with an NTU email and password, logs in with them, or creates a one-click demo account in demo mode; administrator-provisioned token accounts exist only through the API.
2. The browser lists persisted market instances and their current outcome probabilities.
3. The participant selects an outcome, buy or sell, and a share quantity.
4. The backend calculates a signed quote, including the trading fee, without changing market state.
5. After confirmation, the browser saves the exact request and a new idempotency key before sending it.
6. The backend rechecks the quote, account, market version, cutoff, balance, holdings, and reserve inside one database transaction.
7. A successful commit updates the ledger, position, inventory, durable receipt, and public outbox event together.
8. After the observation window, evidence arrives from the market's resolution authority (the administrator, the creator's signed statement, or the external resolver) and the background worker finalizes and settles the instance in bounded batches.

If a trade response is interrupted, the browser retains the same request and key. Retrying either executes it once or retrieves the already committed receipt.

## System shape

```text
React browser
    |
    | JSON over HTTP / Server-Sent Events
    v
Rust Axum API ----> authentication and quote verification
    |              execution and resolution services
    |              pure LMSR arithmetic
    v
PostgreSQL ------> accounts, ledger, markets, positions, trades
                   evidence, settlement claims, audit, outbox
    ^
    |
background worker closes, spawns series brackets, asks external resolvers, observes demo markets, and settles claims
```

The API and worker can run in the same process, as they do by default. PostgreSQL transactions, locks, uniqueness constraints, and immutable-record triggers protect shared state.

## Market lifecycle

```text
OPEN -> CLOSED -> RESOLVING -> RESOLVED
                         \----> VOIDED
```

- `OPEN`: trading is possible until the authoritative database time reaches `close_ms`, unless suspended.
- `CLOSED`: trading has ended; the instance may be waiting for complete evidence or its finalization time.
- `RESOLVING`: the result is fixed and settlement claims are being credited.
- `RESOLVED`: a winning outcome was settled and unused reserve was released.
- `VOIDED`: the void policy was settled and unused reserve was released.

The API can display an effectively closed state as soon as the cutoff passes, even before the worker persists the `OPEN -> CLOSED` transition.

## Important safeguards

- Account tokens are returned in plaintext only at account creation and login; PostgreSQL stores their SHA-256 hashes, and each login invalidates the previous token.
- Quotes are signed, bound to an account and market version, and expire after at most 15 seconds.
- Idempotency keys prevent duplicate trade effects after retries.
- User and reserve balances cannot become negative.
- Every movement has a source, destination, amount, kind, reference, and timestamp.
- Published market rules and terminal results are protected from later editing.
- Evidence and settlement records are append-only.
- Settlement claims are unique per account and instance and can resume after a restart.
- Reconciliation checks stored balances, inventory, outstanding obligations, and total issued units.

## Data-source assumption

Live evidence adapters are not part of the current build. Candidate sources named during planning, such as NEA and OmniBus, are treated as future adapter inputs rather than current dependencies. Weather, bus, crowd, and attendance integrations will have practical challenges such as access permission, stable identifiers, missing readings, corrections, completeness, and proving that an observation matches the published window. For current development, the project assumes that an authorized adapter can eventually provide the normalized evidence required by each rule. The local demo uses clearly labelled simulated evidence and must never be presented as live data.

## Platform direction

PolyNTU has grown from an administrator-published demonstration into a campus platform where verified NTU members publish markets and earn a share of the trading fees. The planned direction is now fully implemented. The full model, with actors, flows, and business rules, is the [use case model](developer/use-cases.md).

- **Accounts**: implemented. Registration and login with an NTU email address and a password, a 10,000-unit welcome gift, and a member-to-creator verification workflow ([ADR 0005](decisions/0005-ntu-accounts-and-creator-roles.md), accepted).
- **Creator-owned series**: implemented. Markets are published as series that occur once or recur in rolling brackets, with a creator-set interval, active period, maximum concurrency, and optional end date; a series without an end date is perpetual. Creators cannot trade in their own markets, and their compensation is the fee share, except on the fee-free welfare markets they choose to publish ([ADR 0006](decisions/0006-market-series-and-recurrence.md), accepted).
- **Resolution authority**: implemented. Fixed at creation: the platform administrator (the default), the creator signing each human resolution with a key the platform never holds, or an external resolver endpoint whose answer PolyNTU validates against the published options. Administrator evidence cannot resolve a creator-signed or resolver-settled market ([ADR 0007](decisions/0007-resolution-authority.md), accepted).
- **Market experience**: implemented. Live quotes and probabilities, joined by a live-refreshing price and volume history chart on every market and a day-long implied-probability view for recurring series.

Every use case in the model is marked existing. The one deferred item is the real bus timing adapter, a live data source rather than platform work.

## Current limitations

- No campus SSO or password recovery; login rotates the single session token.
- The creator resolution signing key is derived from the account password, so any browser where the creator signs in can resolve, but a lost password voids the market at its published deadline because no password recovery exists; the copy in the browser's local storage is only a cache.
- The shared administrator token carries no per-action identity; administrators resolve only platform-authority markets, and the seeded demo administrator (`admin@ntu.edu.sg`, password `admin`) exists only in demo-mode databases.
- No request-rate limiting or public deployment hardening.
- No live provider adapters or independent verification of manual observations; the real bus timing adapter remains deferred, and resolver-based resolution trusts the configured endpoint.
- No outbox retention or compaction beyond the bracket purge.
- Settled brackets of recurring series are purged 24 hours after their evidence deadline, together with their trades, positions, evidence, and settlement credits, so old recurring-series data does not pile up; one-time markets and the append-only ledger and audit history are kept forever.
- No resting orders, short selling, leverage, real-money payments, or budget-to-share inversion.
- A scripted headless-browser end-to-end test drives the real UI in CI; interactive browser control stays prohibited, so visual layout, keyboard focus, and aesthetics still rely on human review.

For implementation details, continue with the [developer guide](developer-guide.md).
