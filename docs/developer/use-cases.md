# PolyNTU use case model

This is the use case model of the planned PolyNTU platform: the actors, the complete register of use cases, the business rules behind them, and detailed flows for the most significant cases. The register marks each case as existing, partial, or new:

- **exists**: implemented and working in the current build;
- **partial**: a smaller or earlier version works in the current build;
- **new**: planned, not built.

Statements about existing behavior are verified against the current build. Everything marked new or partial describes the plan, recorded in [ADR 0006](../decisions/0006-market-series-and-recurrence.md) (market series and recurrence) and [ADR 0007](../decisions/0007-resolution-authority.md) (resolution authority). [ADR 0005](../decisions/0005-ntu-accounts-and-creator-roles.md) (NTU accounts and creator roles) is implemented; its use cases are marked exists.

## Actors

| Actor | Kind | Description |
|---|---|---|
| Visitor | Primary | An unauthenticated person who registers. UC-1 turns a Visitor into a Trader. |
| Trader | Primary | An account holder who browses markets, views quotes and probabilities, trades outcome shares, and tracks a portfolio. |
| Market Creator | Primary | A verified trader who also defines markets. The role generalizes Trader, with one restriction: a creator cannot trade in their own markets, and the fee share is their compensation. |
| Platform Admin | Primary | The operator, reached through the shared token or an admin-role account session (the seeded demo administrator in demo mode). Verifies creators, suspends instances, grants units from the treasury, and runs reconciliation. Today it also creates every instance and records evidence; by design it cannot resolve creator-owned markets. |
| External Resolver | Secondary system | An automated external API the settlement worker calls at finalize time, for example a bus timing service or a queue counter. Secondary actor in UC-13. |
| Scheduler | Internal system | The background process that spawns bracket instances for recurring series on a rolling schedule (UC-12). |
| Settlement Worker | Internal system | The background process that closes instances, resolves them, settles and pays out claims, and voids on missing or invalid evidence (UC-13, UC-15 to UC-17). |

## Diagram

![Use case diagram](../diagrams/use-case.png)

The source is `docs/diagrams/use-case.puml`; re-render it with `scripts/render-diagrams.ps1` (PlantUML, Smetana layout). Color legend: green elements exist today, yellow are partial, orange are new or planned. Scheduler and Settlement Worker are internal system actors, so they are drawn inside the PolyNTU boundary; the External Resolver sits in its own box that names the request and response contract it must honour.

## Use case register

| Area | ID | Use case | Primary actor | Status |
|---|---|---|---|---|
| Account and access | UC-1 | Create an account with an NTU email | Visitor | exists |
| Account and access | UC-2 | Log in with email and password | Trader | exists |
| Account and access | UC-3 | Receive the 10,000-unit welcome gift | Trader | exists |
| Account and access | UC-4 | View portfolio, trades, and pending receipts | Trader | exists |
| Account and access | UC-5 | Browse and search markets | Trader | exists |
| Creator lifecycle | UC-6 | Request creator verification | Trader | exists |
| Creator lifecycle | UC-7 | Approve or reject a creator request | Platform Admin | exists |
| Market definition | UC-8 | Create a one-time market | Market Creator | new |
| Market definition | UC-9 | Create a recurring or perpetual market | Market Creator | new |
| Market definition | UC-10 | Configure automatic resolution | Market Creator | new |
| Market definition | UC-11 | Declare human, creator-only resolution | Market Creator | new |
| Instance lifecycle | UC-12 | Spawn the next bracket on a rolling schedule | Scheduler | new |
| Instance lifecycle | UC-13 | Resolve automatically via the external resolver API | Settlement Worker | new |
| Instance lifecycle | UC-14 | Submit a signed human resolution | Market Creator | new |
| Instance lifecycle | UC-15 | Settle and pay out | Settlement Worker | exists |
| Instance lifecycle | UC-16 | Split the fee pot with the creator | Settlement Worker | exists |
| Instance lifecycle | UC-17 | Void on missing or invalid evidence | Settlement Worker | exists |
| Trading | UC-18 | View live quotes and probabilities | Trader | exists |
| Trading | UC-19 | View the price and volume history chart | Trader | new |
| Trading | UC-20 | Place a trade | Trader | exists |
| Trading | UC-21 | View the day-long probability visualization | Trader | new |
| Admin and ops | UC-22 | Suspend an instance | Platform Admin | exists |
| Admin and ops | UC-23 | Grant units from the treasury | Platform Admin | exists |
| Admin and ops | UC-24 | Run reconciliation | Platform Admin | exists |

Notes:

- Visitor is the unauthenticated role that UC-1 turns into a Trader.
- UC-3: the 10,000-unit gift applies to registered accounts; the one-click demo account keeps its 1,000-unit grant as a development fixture.
- UC-20 exists today; its creator self-trading ban is a planned alternative flow (see the detailed description).
- The diagram also shows four relationship use cases that this register does not number: Validate the response against the options (included by UC-13), Verify the ed25519 signature (included by UC-14), Charge the 25 bps trading fee (included by UC-20), and Reject the creator's own trades (extends UC-20).

## Business rules

1. Accounts require an NTU email (existing; [ADR 0005](../decisions/0005-ntu-accounts-and-creator-roles.md)). The address must match `^[^@\s]+@([a-z0-9-]+\.)*ntu\.edu\.sg$` (case-insensitive), so `billy@ntu.edu.sg` and `billy@scse.ntu.edu.sg` pass and anything else is rejected.
2. Registered accounts receive a 10,000-unit welcome gift from the treasury; the demo account keeps its 1,000-unit grant (existing; ADR 0005). Total issuance rises from 1M to 1B units, applied as a second idempotent bootstrap transfer rather than a migration, so the gift budget is not exhausted after 100 users.
3. Every trade pays a 25-basis-point fee on the LMSR amount, included in the quoted all-in amount, accumulated in the market reserve, and split 50/50 between the creator and the treasury at settlement (existing; [ADR 0004](../decisions/0004-trade-fees.md)).
4. Creators cannot trade in their own markets; the fee share is their compensation.
5. A perpetual market is a recurring market with no end date: it recurs forever with automatically refreshing resolution times.
6. Rolling spawn: a new bracket instance is created every interval while the live count is below the maximum concurrency; the covered horizon is maximum concurrency × interval.
7. Recurrence only spawns brackets inside the creator-set active period. A bus series, for example, runs 06:00 to 24:00 because buses do not run at midnight.
8. Market reserves remain treasury-funded; creators contribute definitions and earn through the fee share, not through deposits.
9. The resolution authority is fixed at market creation: the market creator (human) or a configured external resolver endpoint (automatic).
10. Human resolution requires a valid ed25519 signature from the key fixed at creation. The private key is held only by the creator's browser; a lost key means the market voids by the published policy.
11. The administrator cannot resolve creator-owned markets, by design.
12. Automatic resolution that is unreachable, malformed, or names an unpublished option at the finalize deadline voids the market.
13. A market offers 2 to 8 direct outcomes (existing).

## Domain entities

| Entity | Status | Purpose |
|---|---|---|
| Account | existing | Identity and balance. Registered accounts carry a unique NTU email, an argon2 password hash, and a role (member, creator, or admin). |
| VerificationRequest | existing | A member's creator request with status and the administrator's decision and reason. |
| MarketSeries | new | A creator-owned definition with options, rule, recurrence rule (interval, active period, maximum concurrency, optional end date), and resolution authority. |
| Instance | extended | One concrete occurrence with its own inventory, reserve, evidence, and result. Extended with a series reference and bracket slot. |
| ResolverConfig | new | The endpoint URL and fixed request contract for automatic authority. |
| ResolutionKey | new | The ed25519 public key fixed at creation for human authority. |
| Trade | existing | The source of price and volume history. |
| LedgerEntry | existing | Every unit movement; includes the fee kind. |
| DayProbability | derived | Computed on request from live brackets, not stored. |

## Detailed descriptions

Full descriptions of the twelve most significant use cases, in ID order. A step marked "(existing)" or "(new)" appears only where one flow mixes both.

### UC-1: Create an account with an NTU email

**Precondition:** none. The visitor holds an NTU-affiliated email address.

**Flow of events:**

1. The visitor submits a display name, an email address, and a password.
2. The server validates the display name (2 to 60 characters, the current rule), the email against the NTU pattern and for uniqueness, and the password against the 12-character minimum.
3. The server creates the account with an argon2id hash of the password.
4. The 10,000-unit welcome gift transfers from the treasury to the new account.
5. The server issues a session token and returns it once.

**Alternative flows:**

- The email fails the NTU pattern: registration is rejected.
- The email is already registered: registration is rejected.
- The password is shorter than 12 characters: registration is rejected.
- The treasury gift budget is exhausted: registration is rejected.
- Development and CI: the current one-click demo account remains available in demo mode.

### UC-2: Log in with email and password

**Precondition:** an account exists.

**Flow of events:**

1. The account holder submits the email address and password.
2. The server verifies the password with argon2.
3. The server issues a fresh bearer session token, returns it once, and stores only its SHA-256 hash. Each login invalidates every previous token: an account holds at most one live session.

**Alternative flows:**

- Unknown email or wrong password: the server returns the same generic rejection for both, so accounts cannot be enumerated.
- Repeated failures: the current build has no rate limiting; rate limiting is deferred.

### UC-6: Request creator verification

**Precondition:** an authenticated member account.

**Flow of events:**

1. The member submits a verification request.
2. The server stores the request as pending.
3. The request becomes visible to the administrator.
4. Once approved through UC-7, the account holds the creator role permanently without re-verifying (creating markets as a creator is UC-8, still planned).

**Alternative flows:**

- The account is already a creator, an administrator, or a demo account without an email: the request is rejected with a specific message.
- A pending request already exists: the request is rejected.

### UC-7: Approve or reject a creator request

**Precondition:** a pending verification request exists; the actor is the administrator.

**Flow of events:**

1. The administrator reviews the pending request.
2. The administrator approves it.
3. The account role becomes creator, permanently, and the decision is recorded in the administrator audit.
4. The requester is notified and keeps the creator role without re-verifying (creating markets as a creator is UC-8, still planned).

**Alternative flows:**

- Reject: the administrator records a reason with the rejection and the requester is notified; the member may apply again.

### UC-8: Create a one-time market

**Precondition:** an authenticated creator.

**Flow of events:**

1. The creator submits a title, a resolution criterion, 2 to 8 options, a close time, an observation window, a finalize deadline, and the resolution authority (an automatic endpoint or human, meaning the creator's own signature).
2. The server validates the submission.
3. The instance is published immutably.
4. Its reserve is subsidized from the treasury.
5. It appears in discovery.

**Alternative flows:**

- Invalid or duplicated options: rejected.
- Inconsistent or past times: rejected.
- Invalid endpoint format for automatic authority: rejected.
- Treasury exhausted: rejected.

### UC-9: Create a recurring or perpetual market

**Precondition:** an authenticated creator.

**Flow of events:**

1. The creator submits what UC-8 requires plus a recurrence rule: an interval (minimum 1 minute), an active period (a daily operating window), a maximum concurrency (cap 50), and an optional end date. The absence of an end date means perpetual.
2. The server validates the submission.
3. The series is created.
4. The scheduler begins rolling spawn (UC-12).

**Alternative flows:**

- Interval out of bounds: rejected.
- Concurrency above the cap: rejected.
- Empty active period: rejected.
- End date before the first spawn: rejected.

### UC-12: Spawn the next bracket on a rolling schedule

**Precondition:** an active recurring series exists; the actor is the Scheduler.

**Flow of events:**

1. On each tick, the scheduler checks whether the live count is below the maximum concurrency and the next slot start falls inside the active period.
2. When both hold, it creates the instance for bracket [T, T+interval) with the series' options and rule.
3. It funds the new instance's reserve from the treasury.
4. It publishes the instance.

**Alternative flows:**

- Next slot outside the active period: skip until the window reopens.
- Series end date reached: stop spawning; the live brackets run out and settle.
- Treasury exhausted: pause spawning and alert the operator.

Worked example: a bus series with a 2-minute interval and maximum concurrency 5 always holds five live brackets covering a rolling 10-minute horizon, and a new bracket appears every 2 minutes.

### UC-13: Resolve automatically via the external resolver API

**Precondition:** the instance is closed and inside its finalize window, and the series has a resolver endpoint.

**Flow of events:**

1. The settlement worker calls the endpoint with the fixed request contract: instance identifier, bracket window, and rule parameters.
2. The response is parsed.
3. The result is checked against the published option set.
4. A valid result is recorded as evidence and settlement proceeds.

**Alternative flows:**

- Endpoint unreachable: retry with backoff until the finalize deadline.
- Response malformed or naming no published option: treated as missing evidence.
- Endpoint reports pending: retry.
- Nothing valid by the deadline: the instance voids per the published policy.

### UC-14: Submit a signed human resolution

**Precondition:** a creator-owned market with human authority. The instance is closed, the creator's browser holds the market's ed25519 private key, and the matching public key was fixed at creation.

**Flow of events:**

1. The creator selects the outcome in the interface.
2. The client signs (instance id, outcome, nonce) with the private key.
3. The server verifies the signature against the stored public key.
4. The resolution is recorded and settlement proceeds.

**Alternative flows:**

- Signature invalid: rejected.
- The submitter is not the creator: rejected.
- The market is not closed: rejected.
- An administrator attempts it: rejected by design; the admin evidence route refuses creator-owned markets and no valid signature exists.
- The private key is lost: resolution is impossible and the instance voids at the deadline.

### UC-19: View the price and volume history chart

**Precondition:** an instance exists and the page is open.

**Flow of events:**

1. The backend returns the historical series: time-bucketed outcome prices and traded volume over the instance lifetime.
2. The chart renders alongside the live probability.
3. Each live trade event appends to the chart through the existing SSE stream, like a stock chart.

**Alternative flows:**

- No trades yet: a flat line at opening prices and zero volume.
- Connection drop: the existing durable SSE cursor replays missed events on reconnect.

### UC-20: Place a trade

**Precondition:** an authenticated account, an open instance, and a sufficient balance (all existing).

**Flow of events:**

1. The account requests a quote: signed, bound to the account and market version, expiring within 15 seconds, and all-in including the 25-basis-point fee (existing).
2. The account confirms.
3. The server rechecks the quote, account, market version, cutoff, balance, holdings, and reserve inside one transaction and executes atomically with idempotency (existing).
4. The receipt is returned and an SSE event is published (existing).

**Alternative flows:**

- Expired quote: rejected (existing).
- Insufficient balance: rejected (existing).
- Closed market: rejected (existing).
- Limit not met: rejected (existing).
- Duplicate delivery: the same receipt is returned (existing).
- The account is the creator of this instance: rejected (new); creators cannot trade their own markets, and the fee share is their compensation.

### UC-21: View the day-long probability visualization

**Precondition:** a recurring series exists with live or settled brackets.

**Flow of events:**

1. The trader opens the series page.
2. The backend aggregates per slot: weighted probability = units bet in that bracket × its LMSR yes price, divided by the sum of the same product over all live brackets.
3. The chart shows the whole active period at interval granularity.
4. Settled slots show the resolved outcome.
5. Live slots refresh through SSE.

**Alternative flows:**

- No live brackets, outside the active period: a settled-only view.
- A single live bracket: its weighted probability is its own price.

## Gap analysis: plan versus current build

| Planned capability | Current build | Size |
|---|---|---|
| Creator-owned market series (one-time, recurring, perpetual) | Administrator-created single-window instances | large |
| Recurrence: interval, active period, maximum concurrency, rolling spawn | None; the bus demo hardcodes a 10-minute window | medium |
| Creator trading ban | None | small |
| Human resolution by creator signature | Evidence flows only through the admin route | medium |
| Automatic resolution via an external API contract | None; live adapters are deferred | medium |
| Price and volume history chart with live refresh | Live probabilities and SSE exist; no volume display or history | medium |
| Day-long probability visualization | None | medium |
| Accounts with NTU email, password login, roles | Shipped ([ADR 0005](../decisions/0005-ntu-accounts-and-creator-roles.md)) | none |
| Creator verification workflow | Shipped ([ADR 0005](../decisions/0005-ntu-accounts-and-creator-roles.md)) | none |
| 10,000-unit welcome gift | Shipped ([ADR 0005](../decisions/0005-ntu-accounts-and-creator-roles.md)) | none |
| Fee share for creators | Shipped ([ADR 0004](../decisions/0004-trade-fees.md)) | none |
| Live quotes and probabilities | Shipped | none |
| Settlement, void policy, reconciliation | Shipped | none |

Method: each planned capability was compared against the current build as verified in the working tree. Size grades relative implementation effort from small to large; none means nothing remains to build.

## Implementation phases

1. Accounts and access (ADR 0005): registration, login, welcome gift, roles, verification workflow. Implemented.
2. Market series (ADR 0006): series entity, recurrence, scheduler, the bus demo as a rolling 2-minute series with maximum concurrency 5, creator trading ban.
3. Resolution authority (ADR 0007): signed human resolution with admin exclusion, the external resolver contract.
4. Market experience: volume statistics, the price and volume history chart, the day-long visualization.

The real bus timing adapter stays deferred until these phases land.

## Related records

- [ADR 0005: NTU accounts and creator roles](../decisions/0005-ntu-accounts-and-creator-roles.md)
- [ADR 0006: Market series and recurrence](../decisions/0006-market-series-and-recurrence.md)
- [ADR 0007: Resolution authority](../decisions/0007-resolution-authority.md)
- [ADR 0004: Trade fees](../decisions/0004-trade-fees.md) (the existing fee and creator split)
- [General overview](../overview.md)
