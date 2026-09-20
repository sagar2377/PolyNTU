# PolyNTU use case model

This is the use case model of the PolyNTU platform: the actors, the complete register of use cases, the business rules behind them, and detailed flows for every registered use case. The register marks each case as existing, partial, or new:

- **exists**: implemented and working in the current build;
- **partial**: a smaller or earlier version works in the current build;
- **new**: planned, not built.

Statements about existing behavior are verified against the current build. Every use case is now marked exists: [ADR 0005](../decisions/0005-ntu-accounts-and-creator-roles.md) (NTU accounts and creator roles), [ADR 0006](../decisions/0006-market-series-and-recurrence.md) (market series and recurrence), [ADR 0007](../decisions/0007-resolution-authority.md) (resolution authority), and the market-experience phase are all implemented. The one deferred item is the real bus timing adapter, a live data source rather than platform work.

## Actors

| Actor | Kind | Description |
|---|---|---|
| Visitor | Primary | An unauthenticated person who registers. UC-1 turns a Visitor into a Trader. |
| Trader | Primary | An account holder who browses markets, views quotes and probabilities, trades outcome shares, and tracks a portfolio. |
| Market Creator | Primary | A verified trader who also defines markets: one-time markets and recurring series. The role generalizes Trader, with one restriction: a creator cannot trade in their own markets, and the fee share is their compensation. |
| Platform Admin | Primary | The operator, reached through the shared token or an admin-role account session (the seeded demo administrator in demo mode). Verifies creators, suspends instances, grants units from the treasury, and runs reconciliation. Today it still creates instances directly and records evidence for platform-authority markets; by design it cannot resolve markets whose authority is fixed to their creator or an external resolver. |
| External Resolver | Secondary system | An automated external API the settlement worker calls at finalize time, for example a bus timing service or a queue counter. Secondary actor in UC-13. |
| Scheduler | Internal system | The background process that spawns bracket instances for recurring series on a rolling schedule (UC-12). |
| Settlement Worker | Internal system | The background process that closes instances, resolves them, settles and pays out claims, and voids on missing or invalid evidence (UC-13, UC-15 to UC-17). |

## Diagram

![Use case diagram](../diagrams/use-case.png)

The source is `docs/diagrams/use-case.puml`; re-render it with `scripts/render-diagrams.ps1` (PlantUML, Smetana layout). Color legend: green elements exist today, yellow are partial, orange are new or planned; every use case is currently green. Scheduler and Settlement Worker are internal system actors, so they are drawn inside the PolyNTU boundary; the External Resolver sits in its own box that names the request and response contract it must honour.

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
| Market definition | UC-8 | Create a one-time market | Market Creator | exists |
| Market definition | UC-9 | Create a recurring or perpetual market | Market Creator | exists |
| Market definition | UC-10 | Configure automatic resolution | Market Creator | exists |
| Market definition | UC-11 | Declare human, creator-only resolution | Market Creator | exists |
| Instance lifecycle | UC-12 | Spawn the next bracket on a rolling schedule | Scheduler | exists |
| Instance lifecycle | UC-13 | Resolve automatically via the external resolver API | Settlement Worker | exists |
| Instance lifecycle | UC-14 | Submit a signed human resolution | Market Creator | exists |
| Instance lifecycle | UC-15 | Settle and pay out | Settlement Worker | exists |
| Instance lifecycle | UC-16 | Split the fee pot with the creator | Settlement Worker | exists |
| Instance lifecycle | UC-17 | Void on missing or invalid evidence | Settlement Worker | exists |
| Trading | UC-18 | View live quotes and probabilities | Trader | exists |
| Trading | UC-19 | View the price and volume history chart | Trader | exists |
| Trading | UC-20 | Place a trade | Trader | exists |
| Trading | UC-21 | View the day-long probability visualization | Trader | exists |
| Admin and ops | UC-22 | Suspend an instance | Platform Admin | exists |
| Admin and ops | UC-23 | Grant units from the treasury | Platform Admin | exists |
| Admin and ops | UC-24 | Run reconciliation | Platform Admin | exists |
| Account and access | UC-25 | Sign out | Trader | exists |

Notes:

- Visitor is the unauthenticated role that UC-1 turns into a Trader.
- UC-3: the 10,000-unit gift applies to registered accounts; the one-click demo account keeps its 1,000-unit grant as a development fixture.
- UC-20 includes its creator self-trading ban alternative flow (see the detailed description).
- The diagram also shows four relationship use cases that this register does not number: Validate the response against the options (included by UC-13), Verify the ed25519 signature (included by UC-14), Charge the 25 bps trading fee (included by UC-20), and Reject the creator's own trades (extends UC-20).

## Business rules

1. Accounts require an NTU email (existing; [ADR 0005](../decisions/0005-ntu-accounts-and-creator-roles.md)). The address must match `^[^@\s]+@([a-z0-9-]+\.)*ntu\.edu\.sg$` (case-insensitive), so `billy@ntu.edu.sg` and `billy@scse.ntu.edu.sg` pass and anything else is rejected.
2. Registered accounts receive a 10,000-unit welcome gift from the treasury; the demo account keeps its 1,000-unit grant (existing; ADR 0005). Total issuance rises from 1M to 1B units, applied as a second idempotent bootstrap transfer rather than a migration, so the gift budget is not exhausted after 100 users.
3. Every trade on a fee-charging market pays a 25-basis-point fee on the LMSR amount, included in the quoted all-in amount, accumulated in the market reserve, and split 50/50 between the creator and the treasury at settlement (existing; [ADR 0004](../decisions/0004-trade-fees.md)). The fee is a market attribute fixed at creation (`fee_charged`, default true): the administrator sets it creating instances directly and creators set it publishing a series. A fee-free market charges nothing, collects no fee, and pays no creator share; bus timing is the welfare example.
4. Creators cannot trade in their own markets; the fee share is their compensation.
5. A perpetual market is a recurring market with no end date: it recurs forever with automatically refreshing resolution times.
6. Rolling spawn: a new bracket instance is created every interval while the live count is below the maximum concurrency; the covered horizon is maximum concurrency × interval. Each spawned bracket's title carries its time window (`{series title} · HH:MM to HH:MM` in Singapore time), so brackets of one series stay distinguishable; a one-time market keeps the creator's title unchanged.
7. Recurrence only spawns brackets inside the creator-set active period, a daily window interpreted in Singapore time. A bus series, for example, runs 06:00 to 23:59 because buses do not run at midnight.
8. Market reserves remain treasury-funded; creators contribute definitions and earn through the fee share, not through deposits.
9. The resolution authority is fixed at market creation: the platform administrator (the default), the market creator (human), or a configured external resolver endpoint (automatic) (existing; [ADR 0007](../decisions/0007-resolution-authority.md)).
10. Human resolution requires a valid ed25519 signature from the key fixed at creation. The signing key is derived from the creator's account password (ADR 0007 amendment), so holding the password is holding the key and any browser where the creator signs in can resolve; a lost password means the market voids by the published policy, since no password recovery exists (existing; ADR 0007).
11. The administrator cannot resolve markets whose authority is fixed to their creator or an external resolver, by design (existing; ADR 0007).
12. Automatic resolution that stays unreachable, malformed, pending, or names an unpublished option through the published evidence deadline voids the market (existing; ADR 0007).
13. A market offers 2 to 8 direct outcomes.
14. Terminal brackets of a recurring series are purged 24 hours after their evidence deadline, together with their trades, positions, evidence, and settlement claims; one-time markets are kept indefinitely. The append-only ledger trail and audit history survive every purge, and balances keep settled units after the corresponding credits vanish from the portfolio history.

## Domain entities

| Entity | Status | Purpose |
|---|---|---|
| Account | existing | Identity and balance. Registered accounts carry a unique NTU email, an argon2 password hash, and a role (member, creator, or admin). |
| VerificationRequest | existing | A member's creator request with status and the administrator's decision and reason. |
| MarketSeries | existing | A published definition with the rule, recurrence rule (interval, active period, maximum concurrency, optional end date), liquidity, and fee policy; one-time or recurring. The resolution authority (administrator, creator key, or resolver endpoint) is fixed at creation. |
| Instance | existing | One concrete occurrence with its own inventory, reserve, evidence, and result; carries a series reference, bracket slot, and fee flag when it belongs to a series. |
| ResolverConfig | existing | The endpoint URL and fixed request contract for automatic authority. |
| ResolutionKey | existing | The ed25519 public key fixed at creation for human authority. |
| Trade | existing | The source of price and volume history. |
| LedgerEntry | existing | Every unit movement; includes the fee kind. |
| DayProbability | existing | Computed on request from live brackets, never stored. |

## Detailed descriptions

Full descriptions of all twenty-five use cases, in ID order.

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
- Demo mode: the login dialog's Demo accounts disclosure also offers one-click sign-in buttons for the seeded bus market creator (`bus@ntu.edu.sg`, the account that owns the welfare bus series, so the creator experience can be demonstrated) and the seeded administrator (`admin@ntu.edu.sg`).

### UC-3: Receive the 10,000-unit welcome gift

**Precondition:** the visitor has submitted a valid registration (UC-1).

**Flow of events:**

1. The server inserts the new member account.
2. In the same transaction it locks the treasury and the new account and checks that the treasury balance covers the gift.
3. The treasury transfers the 10,000-unit welcome gift to the account as a `grant` ledger transfer with the fixed reference `grant:{account id}`.
4. The registration response returns the funded balance, and the gift stays visible in the ledger trail of both accounts.

**Alternative flows:**

- The treasury balance is below the gift: registration is rejected with a conflict and no account remains.
- Demo and administrator-provisioned accounts receive the smaller 1,000-unit grant instead (UC-23); only registered accounts receive the welcome gift.

### UC-4: View portfolio, trades, and pending receipts

**Precondition:** an authenticated account.

**Flow of events:**

1. The trader opens the portfolio page; the browser requests the portfolio and the private trade history together and refreshes both every five seconds.
2. The portfolio returns the account with its available balance, every positive position (market, outcome, share count, current state or result, and whether it was redeemed), and one settlement credit per settled market.
3. The trade history lists the account's own trades newest first: market and outcome, side, shares, all-in amount, and time.
4. When a trade response was interrupted, the browser has kept the exact request body and idempotency key; the app shows a pending-trade notice with a Resume trade control, and the retry either executes it once or retrieves the committed receipt.

**Alternative flows:**

- The saved pending trade belongs to another account: the app explains that the account must sign in to retrieve its receipt.
- One offset paginates positions, credits, and trades together, so a page can be sparse in one list and full in another; Next is disabled only when all three lists return fewer than 100 rows.
- A settled recurring bracket past the retention window: its positions and credits disappear from the lists while the balance keeps the units (business rule 14).

### UC-5: Browse and search markets

**Precondition:** none; discovery is public.

**Flow of events:**

1. The trader opens the markets page; the browser loads up to 100 live instances plus the first page of each history class and refreshes them every ten seconds.
2. The grid lists every market bracket directly, live markets only, soonest to close first; each card opens the bracket's market page, which carries the whole series view.
3. Closed, voided, and resolved markets are collapsed into a history panel with Closed, Voided, and Resolved tabs that default to hidden; opening a tab lists its markets, most recently closed first, and clicking a card opens that market page.
4. Paging follows the collapse: with every tab hidden there is no pager, and each open tab pages its own class independently.
5. Category buttons narrow the search to one category; the filter applies client-side to the current page, across the live grid and the history tabs alike.
6. Each card shows the category, effective state, up to three outcomes with their current probabilities, the data-mode label, and the Singapore close time; a card opens the market page.

**Alternative flows:**

- No market on the current page matches the category: an empty-state message appears; the filter does not fetch further pages.
- Fewer than 100 rows arrive: Next is disabled, because the server returns no total count.

### UC-6: Request creator verification

**Precondition:** an authenticated member account.

**Flow of events:**

1. The member submits a verification request.
2. The server stores the request as pending.
3. The request becomes visible to the administrator.
4. Once approved through UC-7, the account holds the creator role permanently without re-verifying (creating markets as a creator is UC-8).

**Alternative flows:**

- The account is already a creator, an administrator, or a demo account without an email: the request is rejected with a specific message.
- A pending request already exists: the request is rejected.

### UC-7: Approve or reject a creator request

**Precondition:** a pending verification request exists; the actor is the administrator.

**Flow of events:**

1. The administrator reviews the pending request.
2. The administrator approves it.
3. The account role becomes creator, permanently, and the decision is recorded in the administrator audit.
4. The requester is notified and keeps the creator role without re-verifying (creating markets as a creator is UC-8).

**Alternative flows:**

- Reject: the administrator records a reason with the rejection and the requester is notified; the member may apply again.

### UC-8: Create a one-time market

**Precondition:** an authenticated creator.

**Flow of events:**

1. The creator submits a title, a resolution criterion, the rule (which derives 2 to 8 options), the evidence source, the liquidity, the fee choice, and one explicit window: close time, observation window, finalize deadline, and evidence deadline.
2. The server validates the submission.
3. The series and its single instance are published immutably; if the instance cannot be funded, the series is removed again.
4. The instance's reserve is subsidized from the treasury.
5. It appears in discovery.

**Alternative flows:**

- The submitter is not a creator: rejected with 403.
- Invalid or duplicated options: rejected.
- Inconsistent or past times: rejected.
- Treasury exhausted: rejected.
- An optional resolution authority (an external resolver endpoint or the creator's own signature) can be fixed at publication; see UC-10 and UC-11 ([ADR 0007](../decisions/0007-resolution-authority.md)).

### UC-9: Create a recurring or perpetual market

**Precondition:** an authenticated creator.

**Flow of events:**

1. The creator submits what UC-8 requires plus a recurrence rule: an interval (1 minute to 1 day), an active period (a daily operating window in Singapore time), operating days (every day, weekdays, or weekends), a maximum concurrency (1 to 50), and an optional end date whose absence means perpetual. The fee choice applies to every bracket, and every spawned bracket's title carries its time window (business rule 6).
2. The server validates the submission.
3. The series is published immutably.
4. The scheduler begins rolling spawn (UC-12), and every bracket's market page shows the schedule and its sibling brackets.

**Alternative flows:**

- Interval out of bounds: rejected.
- Concurrency above the cap: rejected.
- Empty or inverted active period: rejected.
- End date too close to leave room for one more slot: rejected.

### UC-10: Configure automatic resolution

**Precondition:** an authenticated creator is publishing a series (UC-8 or UC-9).

**Flow of events:**

1. The creator chooses the external API as the resolution authority and supplies the endpoint URL.
2. The create form states the contract: PolyNTU posts the instance identifier, the bracket and observation window, and the rule; the endpoint must answer with exactly one published outcome identifier or pending.
3. The server validates the endpoint (https, plain http on loopback only for local adapters) and fixes it at publication as part of the immutable series definition.
4. From each bracket's finalize window the settlement worker calls the endpoint (UC-13); a valid answer settles, and pending, malformed, or unreachable answers retry every tick until the published deadline.

**Alternative flows:**

- Insecure endpoint (plain http off loopback): rejected at publication.
- No valid answer by the evidence deadline: the bracket voids (UC-17).

### UC-11: Declare human, creator-only resolution

**Precondition:** an authenticated creator with a registered email account is publishing a series.

**Flow of events:**

1. The creator chooses their own signature as the resolution authority.
2. The browser uses the cached signing keypair derived from the account password, or asks for the password once and derives the keypair in-browser; the password never leaves the browser.
3. Only the public key is published with the series, fixed at creation as part of the immutable definition, and the derived keypair is cached for later resolutions.
4. The administrator is excluded from resolving this series by design (business rule 11); each closed bracket is later resolved by the creator's signature (UC-14).

**Alternative flows:**

- The cached key is missing: the form asks for the password, derives the keypair in-browser, and caches it after publication.
- The account has no email, for example a demo account: publishing a signed market is refused.
- The password is lost: the key cannot be re-derived and the affected markets void at their deadlines, because no password recovery exists.

### UC-12: Spawn the next bracket on a rolling schedule

**Precondition:** an active recurring series exists; the actor is the Scheduler.

**Flow of events:**

1. On each tick, the scheduler walks every grid slot strictly after now, up to maximum concurrency slots ahead.
2. It skips slots outside the active period, slots past the series end date, and slots that already have a bracket.
3. For each remaining slot it creates the instance for bracket [T, T+interval) with the series' rule, funds the new instance's reserve from the treasury, and publishes it. The spawned bracket's title is `{series title} · HH:MM to HH:MM` (Singapore time), carrying its [T, T+interval) window so brackets of one series stay distinguishable.
4. A recurring series ends when its end date has passed and no non-terminal bracket remains; a one-time series ends when its single instance settles. A settled bracket whose evidence deadline passed more than 24 hours ago is purged with its whole subtree (business rule 14); the series row itself remains.

**Alternative flows:**

- Next slot outside the active period: skipped until the window reopens.
- Series end date reached: spawning stops; the live brackets run out and settle.
- A spawn fails, for example when the treasury is exhausted: the failure is logged per series and retried on the next tick.

Worked example: a bus series with a 2-minute interval and maximum concurrency 5 always holds five live brackets covering a rolling 10-minute horizon, and a new bracket appears every 2 minutes.

### UC-13: Resolve automatically via the external resolver API

**Precondition:** the instance is closed and inside its finalize window, and the series has a resolver endpoint.

**Flow of events:**

1. The settlement worker calls the endpoint with the fixed request contract: instance identifier, bracket window, and rule parameters.
2. The response is parsed.
3. The result is checked against the published option set.
4. A valid result is recorded as evidence and settlement proceeds.

**Alternative flows:**

- Endpoint unreachable: retried on every worker tick (one second) until the published evidence deadline.
- Response malformed or naming no published option: treated as missing evidence and retried.
- Endpoint reports pending: retried.
- Nothing valid by the deadline: the instance voids per the published policy.
- A demo clock jump skips the whole ask window: for simulated instances the deterministic answer is recorded as a replay instead of voiding; manual instances keep the hard deadline.

The demo bus series is the built-in example: the platform serves its own adapter at `POST /api/v2/resolvers/ntu-bus`, which asks the live NTU Bus API provider (`provider/ntubus`, watching the Omnibus feed) first and falls back to the deterministic simulated feed only when the provider stays without a definitive answer; the recorded evidence names the path that answered. Resolver-authority instances never fall back to the platform's automatic simulated evidence, so a broken resolver is visible instead of masked.

### UC-14: Submit a signed human resolution

**Precondition:** a creator-owned market with human authority. The instance is closed, the creator can derive the market's ed25519 signing key from the account password (the browser caches it after login), and the matching public key was fixed at creation.

**Flow of events:**

1. The creator selects the outcome in the interface.
2. The client signs (instance id, outcome, nonce) with the private key.
3. The server verifies the signature against the stored public key.
4. The resolution is recorded and settlement proceeds.

**Alternative flows:**

- Signature invalid: rejected.
- The submitter is not the creator: rejected.
- The market is not closed: rejected.
- An administrator attempts it: rejected by design; the admin evidence route refuses markets with creator or resolver authority, and no valid signature exists.
- The password is lost: the signing key cannot be re-derived, resolution is impossible, and the instance voids at the deadline (no password recovery exists).

### UC-15: Settle and pay out

**Precondition:** a closed instance has reached its finalize window; the actor is the Settlement Worker.

**Flow of events:**

1. The worker selects up to 100 actionable instances per tick, so markets waiting for evidence cannot starve ones whose results are ready, and settles independent instances concurrently in bounded chunks while each instance stays serial under its row lock.
2. It locks the instance and fixes the evaluated result of the latest evidence revision, moving the instance to resolving; with no result it waits until the evidence deadline and then voids (UC-17).
3. It credits up to 100 unsettled accounts per batch: each winning share pays one unit from the reserve, recorded as a unique settlement claim per account and instance.
4. With no unclaimed positive positions left, the worker releases the unused reserve to the treasury, splits the accumulated fee pot (UC-16), and marks the instance resolved or voided.

**Alternative flows:**

- A credit fails: the batch transaction rolls back and the next tick retries; committed claims are never repeated, so settlement resumes from the accounts without a claim.
- More than 100 accounts hold shares: later batches continue until every account is credited.
- The reserve cannot cover a credit: settlement stops with the invariant error rather than underpaying.

### UC-16: Split the fee pot with the creator

**Precondition:** an instance is settling (UC-15) and its recorded trades carried the 25-basis-point fee.

**Flow of events:**

1. The worker sums the fee component of the instance's recorded trades; the fees accumulated inside the reserve as trading happened.
2. With a recorded market creator, half the pot, rounded down to the microcredit, transfers from the reserve to the creator as a `fee` ledger transfer.
3. The treasury receives the remainder, including any odd microcredit, as its own `fee` transfer.

**Alternative flows:**

- Platform-created market with no recorded creator: the whole pot is treasury revenue.
- Fee-free market: no pot exists, nothing is split, and no creator share is paid.

### UC-17: Void on missing or invalid evidence

**Precondition:** a closed instance has passed its finalize window; the actor is the Settlement Worker.

**Flow of events:**

1. No complete final evidence has been selected by the published evidence deadline, or the rule itself evaluated to a void, for example a final election with no winner.
2. The worker fixes a void result with the recorded reason and moves the instance to resolving.
3. Each share redeems at 1/n units: an account's holdings are aggregated across all outcomes and the credit is rounded down to a whole microcredit.
4. Claims are credited from the reserve as in a winner settlement, the unused reserve is released, and the instance becomes voided.

**Alternative flows:**

- Complete final evidence arrives before the deadline: its evaluated result settles instead and the void path never runs.
- Incomplete evidence, for example a partial observation or a non-final election state, waits rather than settling early; the deadline alone decides.

### UC-18: View live quotes and probabilities

**Precondition:** the market page is open. Probabilities are public; a personal quote requires a signed-in account.

**Flow of events:**

1. The market page shows every outcome's current probability as a percentage, with the note that prices reflect trading activity rather than a provider forecast.
2. Each market event on the SSE stream refreshes the snapshot, with a five-second fallback poll, and the price history chart refreshes with it.
3. A signed-in trader picks an outcome, a side, and a share quantity and previews the trade: the server returns the exact all-in cost or proceeds with the fee reported separately, the average price per share, the probability after the trade, and an expiry of at most 15 seconds.
4. The preview is signed and bound to the account and the market version; it reserves nothing, and a changed price requires a new preview.

**Alternative flows:**

- Not signed in: the panel says to sign in from the top right to preview and place a trade.
- The market is suspended or closed: the preview form is disabled.
- The account created this market: the quote is rejected, because creators cannot trade their own markets.

### UC-19: View the price and volume history chart

**Precondition:** an instance exists and the page is open.

**Flow of events:**

1. The backend returns the historical series: time-bucketed outcome prices and traded volume, reconstructed by replaying recorded trades from the opening inventory.
2. The chart renders alongside the live probability, one line per outcome plus a volume histogram.
3. Each market event on the existing SSE stream triggers a full history refresh (with a five-second fallback poll), like a stock chart.

**Alternative flows:**

- No trades yet: a flat line at opening prices and zero volume.
- Connection drop: the existing durable SSE cursor replays missed events on reconnect.

### UC-20: Place a trade

**Precondition:** an authenticated account, an open instance, and a sufficient balance.

**Flow of events:**

1. The account requests a quote: signed, bound to the account and market version, expiring within 15 seconds, and all-in including the 25-basis-point fee on fee-charging markets.
2. The account confirms.
3. The server rechecks the quote, account, market version, cutoff, balance, holdings, and reserve inside one transaction and executes atomically with idempotency.
4. The receipt is returned and an SSE event is published.

**Alternative flows:**

- Expired quote: rejected.
- Insufficient balance: rejected.
- Closed market: rejected.
- Limit not met: rejected.
- Duplicate delivery: the same receipt is returned.
- The account is the creator of this instance: rejected; creators cannot trade their own markets, and the fee share is their compensation.

### UC-21: View the day-long probability visualization

**Precondition:** a recurring series exists with live or settled brackets.

**Flow of events:**

1. The trader opens any bracket's market page; the day view appears on every bracket page of a binary series.
2. The backend aggregates per slot: weighted probability = the sum over live brackets of (units bet in that bracket × its first-outcome price), divided by the total units bet across those brackets; null until a live bracket has traded.
3. The view lists the brackets vertically, each row a horizontal bar whose length is that slot's first-outcome probability, with the slot's time window, percentage, and traded volume.
4. Slots show their actual implied probability, so an untraded series reads as a flat column of opening probabilities; settled slots are not pinned.
5. Live slots refresh through the market page's five-second reload.

**Alternative flows:**

- No live bracket has traded volume yet: the headline says the weighted probability appears with the first trade.
- A single live bracket: its weighted probability is its own price.

### UC-22: Suspend an instance

**Precondition:** an open instance exists; the actor is the administrator.

**Flow of events:**

1. The administrator submits a suspension or resumption with a reason of 5 to 1,000 characters.
2. The server locks the instance, flips the suspended flag, and advances the version.
3. The decision is recorded in the administrator audit and announced as a suspension event on the market's stream.
4. While suspended, quotes and trades are refused; the published definition, times, and result policy are untouched.

**Alternative flows:**

- The instance is not open: rejected with a conflict.
- The reason is missing or outside the bounds: rejected.

### UC-23: Grant units from the treasury

**Precondition:** the actor is the administrator; the treasury holds units.

**Flow of events:**

1. The administrator provisions a new account by submitting a display name.
2. The server validates the name (2 to 60 characters, no control characters), creates the account, and returns its token once.
3. After checking the budget, the treasury transfers the 1,000-unit grant to the new account in the same transaction.

**Alternative flows:**

- The treasury balance is below the grant: rejected with a conflict and no account remains.
- In demo mode the one-click demo endpoint performs the same provisioning for any visitor with the same grant.
- The provisioned account carries no email, password, or role: it cannot log in, and with the token sign-in path gone from the browser its token is the only way in, so the token must be kept.

### UC-24: Run reconciliation

**Precondition:** the actor is the administrator.

**Flow of events:**

1. The administrator requests reconciliation.
2. The server opens one repeatable-read, read-only snapshot and checks four invariants: every account balance equals the sum of its ledger entries; every instance inventory equals the sum of its positions per outcome; every reserve covers its remaining liabilities, being the maximum outstanding outcome while unresolved, the unclaimed winning positions once a winner is fixed, and the aggregated fractional credits once a void is fixed; and the total balance across all accounts is zero.
3. The response reports the error counts and the net total, and ok is true only when all are zero.

**Alternative flows:**

- Any count is nonzero or the net total differs from zero: ok is false and the counts name the area; reconciliation detects divergence but repairs nothing.

### UC-25: Sign out

**Precondition:** a signed-in browser session.

**Flow of events:**

1. The participant presses Sign out in the header's account chip.
2. The browser drops the stored session token and the cached signing key; signing in again re-derives the key from the password.
3. The interface returns to the guest state with the account entry dialog available.

**Alternative flows:**

- The server session token itself stays valid until the next login of that account rotates it; signing out in one browser does not revoke a token stolen elsewhere.
