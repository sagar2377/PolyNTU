# PolyNTU general overview

## Purpose

PolyNTU is a campus-focused prediction-market demonstration. It lets participants express beliefs about future, objectively resolvable campus events by trading outcome shares with simulated units. It is an academic software project, not a real-money betting or payment system.

The current application deliberately uses direct outcomes rather than the call/put options and Black–Scholes model found in the archived prototype. Every open market has a fixed question, outcomes, observation window, evidence source, deadline, and resolution policy.

## Core concepts

| Term | Plain-language meaning |
|---|---|
| Template | A recurring or reusable market definition used to group instances. |
| Instance | One concrete occurrence with its own times, outcomes, inventory, reserve, evidence, and result. |
| Outcome share | A simulated claim on one mutually exclusive result. |
| Probability | The current marginal LMSR price shown as a percentage. It reflects trading, not a provider forecast. |
| Quote | A short-lived preview of the exact cost or proceeds for one proposed trade. |
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

1. A participant creates a demo account or receives an administrator-provisioned account token.
2. The browser lists persisted market instances and their current outcome probabilities.
3. The participant selects an outcome, buy or sell, and a share quantity.
4. The backend calculates a signed quote without changing market state.
5. After confirmation, the browser saves the exact request and a new idempotency key before sending it.
6. The backend rechecks the quote, account, market version, cutoff, balance, holdings, and reserve inside one database transaction.
7. A successful commit updates the ledger, position, inventory, durable receipt, and public outbox event together.
8. After the observation window, evidence is recorded and the background worker finalizes and settles the instance in bounded batches.

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
background worker closes, observes demo markets, and settles claims
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

- Account tokens are returned in plaintext only at provisioning; PostgreSQL stores their SHA-256 hashes.
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

## Current limitations

- No campus SSO, account recovery, or token rotation.
- One administrator credential performs both administration and resolution duties.
- No request-rate limiting or public deployment hardening.
- No live provider adapters or independent verification of manual observations.
- No outbox retention or compaction.
- No resting orders, short selling, leverage, real-money payments, or budget-to-share inversion.
- No visual-browser verification was performed by the development agent because project instructions prohibit browser automation.

For implementation details, continue with the [developer guide](developer-guide.md).
