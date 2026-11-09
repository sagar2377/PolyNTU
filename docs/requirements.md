# PolyNTU — Requirements Specification

Authors: Ignatius Justin Low, Luo Jun Yong, Anshul Sharma, Cao Yu Xuan, Chen Wei Feng

**Scope of this version:** the platform as built and demonstrated in Lab 5 (repository `master`, commit `ad330cd`): accounts and access, market browsing, trading, the creator lifecycle, market definition (one-time and recurring), automatic and signed resolution, settlement and administration. Features described in the earlier draft that were not built (GPS geofencing, crowd confirmations and photo reports, reputation, leaderboard, password reset by email) are removed; a note at the end of the document lists them. The use case model and its full descriptions are in `docs/developer/use-cases.md`.

---

## 3.2.1 Target Users

| Persona | Role in the system | Motivation |
|---|---|---|
| **Visitor** | Unauthenticated person; can browse markets and view probabilities, and can register an account | Wants to see what is on offer before signing up |
| **Student Trader** | Registered NTU account holder; previews and places trades with virtual units, tracks a portfolio | Entertainment, bragging rights, a better read on campus events such as bus timings |
| **Market Creator** | A Student Trader whose creator request was approved by the administrator; defines one-time and recurring markets and may resolve them by signature. A creator cannot trade in their own markets | Wants to run questions of interest to students and earn half of the trading-fee pot of their markets |
| **Platform Admin** | The operator; verifies creators, suspends markets, grants units, runs reconciliation. Cannot resolve any market | Keep the platform trustworthy and running |
| **Scheduler (system actor)** | Automated job that spawns the next bracket of a recurring series on a rolling schedule | Keeps the market list populated without manual effort |
| **Settlement Worker (system actor)** | Automated job that closes, resolves, settles and voids markets | Pays out accurately and exactly once |
| **External Resolver (secondary system)** | An API that answers which outcome occurred, for example the campus bus timing service | Supplies evidence for automatically resolved markets |

### Elicitation approach
Part of the team played "customer" (NTU students who would use the system) while the rest acted as the development team eliciting requirements. Representative statements from that exercise, and the requirement each one drove:

- *"I don't want to type an exact number if I'm not sure — let me just pick from a few options."* → markets offer a small set of discrete outcomes, 2 to 8 per market (FR-5.3).
- *"What if nobody can confirm what happened?"* → a market with no valid evidence by its deadline is voided and holders are refunded (FR-6.8).
- *"This can't actually be gambling with real money or I won't touch it."* → virtual units only, with no conversion to money (NFR-6).

These statements are a sample of the exercise, not a complete log. A fourth statement, about disputes between people who were physically at a location, drove the geofence design that was not built, and is removed with it.

---

## 3.2.2 / 3.2.3 Functional Requirements

Each requirement is atomic (one testable behavior), uses "shall," and states actor → input → output. Thresholds are quantified. The ID is the traceability key for test cases; the last column of the traceability matrix maps each group to use cases.

### FR-1 — Account & Access
| ID | Requirement |
|---|---|
| FR-1.1 | The system shall allow a visitor to register by submitting a display name (2 to 60 characters), an email address and a password of at least 12 characters, producing a unique member account and a session token. |
| FR-1.2 | The system shall reject registration if the email address does not end in `ntu.edu.sg` or a subdomain of it (for example `@ntu.edu.sg` or `@scse.ntu.edu.sg`). |
| FR-1.3 | The system shall reject registration if the email address is already registered. |
| FR-1.4 | The system shall transfer a 10,000-unit welcome gift from the treasury to each newly registered account in the same transaction as the registration. |
| FR-1.5 | The system shall reject registration, creating no account, if the treasury balance is below the welcome gift. |
| FR-1.6 | The system shall allow a registered user to log in with email and password, producing a session token that replaces any earlier token of that account. |
| FR-1.7 | The system shall return the same rejection message for an unknown email and for a wrong password. |
| FR-1.8 | The system shall allow a signed-in user to sign out, which removes the stored session token and cached signing key from the browser. |

### FR-2 — Market Browsing
| ID | Requirement |
|---|---|
| FR-2.1 | The system shall list live markets to any visitor without sign-in, soonest closing first, up to 100 per page. |
| FR-2.2 | The system shall show on each market card its category, state, up to three outcomes with their current probabilities, data-mode label and close time in Singapore time. |
| FR-2.3 | The system shall allow a user to narrow the displayed markets to one category (weather, bus, elections, queue/crowd, attendance or other). |
| FR-2.4 | The system shall list closed, voided and resolved markets in separate history tabs, hidden by default, each paged independently. |
| FR-2.5 | The system shall allow a user to open a market to a detail page showing every outcome with its current probability and the market's resolution criterion as text. |
| FR-2.6 | The system shall refresh the market list every 10 seconds and refresh an open market page on each market event, with a fallback poll every 5 seconds. |

### FR-3 — Trading
| ID | Requirement |
|---|---|
| FR-3.1 | The system shall allow a signed-in user to preview a buy or sell of a chosen number of shares of one outcome of an open market. |
| FR-3.2 | The system shall compute the preview with the LMSR pricing function and return the all-in amount (fee shown separately), the average price per share and the probability after the trade. |
| FR-3.3 | The system shall issue each preview as a signed quote bound to the account and the market version, valid for at most 15 seconds, that reserves nothing. |
| FR-3.4 | The system shall execute a confirmed trade atomically, re-checking the quote, account, market version, close time, balance, holdings and reserve in one transaction. |
| FR-3.5 | The system shall reject a buy if the account balance is below the all-in amount. |
| FR-3.6 | The system shall reject a sell of more shares than the account holds. |
| FR-3.7 | The system shall reject a quote or trade on a market that has closed or is suspended. |
| FR-3.8 | The system shall reject a quote or trade by the creator of that market. |
| FR-3.9 | The system shall charge a 25 basis-point fee on each trade of a fee-charging market, included in the all-in amount and accumulated in the market reserve. |
| FR-3.10 | The system shall return the original receipt, without executing again, when a trade is delivered more than once with the same idempotency key. |
| FR-3.11 | The system shall keep the request of an interrupted trade in the browser and offer a "Resume trade" control that retries it or retrieves the committed receipt. |
| FR-3.12 | The system shall publish an event after every executed trade so that other viewers of the market see updated probabilities. |
| FR-3.13 | The system shall show a price and volume history chart for a market, refreshed with each market event. |
| FR-3.14 | The system shall show, for a recurring series, a day-long view of each live bracket's probability and traded volume. |

### FR-4 — Portfolio & Wallet
| ID | Requirement |
|---|---|
| FR-4.1 | The system shall show a signed-in user their available balance, every positive position (market, outcome, shares, state or result) and their settlement credits. |
| FR-4.2 | The system shall show a signed-in user their own trades, newest first, 100 per page, and no other user's trades. |
| FR-4.3 | The system shall refresh the portfolio and trade history every 5 seconds. |
| FR-4.4 | The system shall record every movement of units as a ledger transfer, so that an account balance equals the sum of its transfers. |

### FR-5 — Creator Lifecycle & Market Definition
| ID | Requirement |
|---|---|
| FR-5.1 | The system shall allow a registered member to request creator verification, and shall reject the request if the account is already a creator or administrator, has no email, or has a pending request. |
| FR-5.2 | The system shall allow the administrator to approve a pending request, giving the account the creator role permanently, or to reject it with a recorded reason. |
| FR-5.3 | The system shall allow a creator to publish a one-time market by submitting a title, resolution criterion, rule (deriving 2 to 8 outcomes), evidence source, liquidity, fee choice, close time, observation window, finalize deadline and evidence deadline. |
| FR-5.4 | The system shall reject a market submission from a non-creator, with duplicate or invalid outcomes, with inconsistent or past times, or when the treasury cannot fund its reserve. |
| FR-5.5 | The system shall allow a creator to publish a recurring or perpetual market by additionally supplying an interval (1 minute to 1 day), a daily active period in Singapore time, operating days, a maximum concurrency (1 to 50) and an optional end date. |
| FR-5.6 | The system shall fix a market's resolution authority at publication as either the creator's signature or an external resolver endpoint, and shall not allow it to change afterwards. |
| FR-5.7 | The system shall accept a resolver endpoint only if it uses https, or plain http on a loopback address. |
| FR-5.8 | The system shall fund each market instance's reserve from the treasury. |

### FR-6 — Market Lifecycle & Resolution
| ID | Requirement |
|---|---|
| FR-6.1 | The system shall, on each scheduler tick, create the next brackets of an active recurring series up to its maximum concurrency, skipping slots outside the active period, past the end date or already filled. |
| FR-6.2 | The system shall title each spawned bracket with its time window in Singapore time. |
| FR-6.3 | The system shall close trading on an instance at its close time. |
| FR-6.4 | The system shall call an automatic market's resolver endpoint from the finalize window, and accept only an answer naming exactly one published outcome. |
| FR-6.5 | The system shall retry a pending, malformed or unreachable resolver answer on every worker tick (one second) until the evidence deadline. |
| FR-6.6 | The system shall accept a human resolution only if the market is closed, the submitter is its creator and the ed25519 signature over the instance, outcome and nonce verifies against the public key fixed at creation. |
| FR-6.7 | The system shall not allow the administrator to resolve any market. |
| FR-6.8 | The system shall void an instance that has no valid evidence by its evidence deadline, redeeming each share at 1 divided by the number of outcomes, rounded down to a whole microcredit. |
| FR-6.9 | The system shall settle a resolved instance by crediting one unit per winning share from the reserve, exactly once per account and instance. |
| FR-6.10 | The system shall split the accumulated fee pot at settlement, half (rounded down) to the market creator and the remainder to the treasury; with no recorded creator, the whole pot goes to the treasury. |
| FR-6.11 | The system shall release the unused reserve to the treasury after settlement. |
| FR-6.12 | The system shall purge a settled recurring bracket 24 hours after its evidence deadline, together with its trades, positions, evidence and settlement claims, while keeping the ledger and audit history. |

### FR-7 — Administration
| ID | Requirement |
|---|---|
| FR-7.1 | The system shall restrict administration functions to a holder of the administrator token or an account with the admin role. |
| FR-7.2 | The system shall allow the administrator to suspend or resume an open instance with a reason of 5 to 1,000 characters, refusing quotes and trades while it is suspended. |
| FR-7.3 | The system shall allow the administrator to provision an account from a display name (2 to 60 characters) with a 1,000-unit grant from the treasury. |
| FR-7.4 | The system shall allow the administrator to run reconciliation, which checks that account balances equal ledger sums, instance inventory equals positions, reserves cover remaining liabilities and total balances sum to zero. |
| FR-7.5 | The system shall record suspensions, verification decisions, evidence events and creator resolutions in an administrator audit log with a timestamp. |

---

## Non-Functional Requirements

The last column states where the requirement is evidenced or what remains a target.

| ID | Category | Requirement | Evidence / status |
|---|---|---|---|
| NFR-1 | Performance | The system shall serve a quote without a database lock and update other viewers' displayed prices within 5 seconds of a trade (event push, with a 5-second fallback poll). | Design in `docs/performance.md`; measured in `docs/verification.md` |
| NFR-2 | Integrity | The system shall keep every account balance, instance inventory and reserve consistent, executing each trade exactly once and serialising trades per market. | Reconciliation (FR-7.4); integration tests |
| NFR-3 | Scalability | The system shall support at least 500 registered users and 50 concurrent live markets. | Design target. Measured throughput and its limits are in `docs/verification.md` and `docs/benchmarks/` |
| NFR-4 | Security (auth) | The system shall store passwords only as argon2id hashes and session tokens only as SHA-256 hashes, never in plaintext. | `docs/developer/security-and-privacy.md` |
| NFR-5 | Security (resolution) | The system shall verify creator resolutions with an ed25519 signature; the private key shall be derived in the browser from the account password and never sent to the server. | ADR 0007; security-and-privacy.md |
| NFR-6 | Compliance | The system shall use only virtual units for all trading, with no mechanism to convert units to money. | Design constraint |
| NFR-7 | Usability | A first-time user shall be able to place a first trade within 3 minutes of registering, without external instructions. | Target; to be confirmed in demo rehearsal |
| NFR-8 | Auditability | The system shall keep an append-only ledger and an administrator audit log that survive every purge. | Business rule 14 in `docs/developer/use-cases.md` |
| NFR-9 | Privacy | The system shall show a user's trade history and portfolio only to that user. | FR-4.2 |
| NFR-10 | Compatibility | The web client shall run in a current desktop browser. | End-to-end test in headless Chrome (CI); other browsers not tested |

---

## 3.2.4 Data Dictionary

| Term | Description | Key Attributes | Relationships |
|---|---|---|---|
| **Account** | A person's identity and balance | `accountId`, `displayName`, `kind`, `email`, `role` (member / creator / admin), `passwordHash`, `tokenHash`, `balance` | Places many Trades; holds many Positions; may request verification |
| **VerificationRequest** | A member's request to become a creator | `requestId`, `status`, `reason`, `requestedAt`, `decidedAt` | Belongs to one Account |
| **MarketSeries** | A published, immutable market definition (one-time or recurring) | `seriesId`, `title`, `resolutionCriterion`, `rule`, `sourceId`, `dataMode`, `liquidityUnits`, `feeCharged`, `recurrence`, `schedule`, `resolutionAuthority`, `resolverConfig`, `resolutionKey`, `state` | Created by one creator Account; groups many MarketInstances |
| **MarketInstance** | One concrete tradable occurrence with its own inventory and reserve | `instanceId`, `state`, `suspended`, `window`, `bracketStart`, `liquidityUnits`, `feeCharged`, `creatorId`, `version`, `result`, `selectedEvidence` | Belongs to at most one series; owns 2 to 8 Outcomes; has one reserve Account |
| **Outcome** | One mutually exclusive possible result of an instance | `outcomeId`, `index`, `label`, `outstandingShares`, `probability` (derived) | Belongs to one instance; referenced by Positions and Trades |
| **Position** | The shares an account holds in one outcome | `quantity` | Links one Account to one Outcome |
| **Trade** | A buy or sell executed against the market maker | `tradeId`, `side`, `quantity`, `allInAmount`, `feeAmount`, `instanceVersion`, `executedAt` | Links one Account to one Outcome |
| **Evidence** | A revision of the information used to resolve an instance | `evidenceId`, `sourceId`, `eventId`, `sourceRevision`, `receivedAt`, `payload`, `evaluatedResult` | Belongs to one instance |
| **SettlementClaim** | The payout credited to one account for one instance | `creditedAmount`, `settledAt` | Unique per Account and instance |
| **LedgerTransfer** | Every movement of units between accounts | `transferId`, `amount`, `kind`, `reference`, `createdAt` | Has one source and one destination Account |

---

## 3.3.1 Use Case Model

The use case model has 25 use cases; the diagram is `docs/diagrams/use-case.png` and every description is in `docs/developer/use-cases.md`.

| ID | Use Case | Primary Actor |
|---|---|---|
| UC-1 | Create an account with an NTU email | Visitor |
| UC-2 | Log in with email and password | Trader |
| UC-3 | Receive the 10,000-unit welcome gift | Trader |
| UC-4 | View portfolio, trades and pending receipts | Trader |
| UC-5 | Browse markets | Trader |
| UC-6 | Request creator verification | Trader |
| UC-7 | Approve or reject a creator request | Platform Admin |
| UC-8 | Create a one-time market | Market Creator |
| UC-9 | Create a recurring or perpetual market | Market Creator |
| UC-10 | Configure automatic resolution | Market Creator |
| UC-11 | Declare human, creator-only resolution | Market Creator |
| UC-12 | Spawn the next bracket on a rolling schedule | Scheduler |
| UC-13 | Resolve automatically via the external resolver API | Settlement Worker (with External Resolver) |
| UC-14 | Submit a signed human resolution | Market Creator |
| UC-15 | Settle and pay out | Settlement Worker |
| UC-16 | Split the fee pot with the creator | Settlement Worker |
| UC-17 | Void on missing or invalid evidence | Settlement Worker |
| UC-18 | View live quotes and probabilities | Trader |
| UC-19 | View the price and volume history chart | Trader |
| UC-20 | Place a trade | Trader |
| UC-21 | View the day-long probability visualization | Trader |
| UC-22 | Suspend an instance | Platform Admin |
| UC-23 | Grant units from the treasury | Platform Admin |
| UC-24 | Run reconciliation | Platform Admin |
| UC-25 | Sign out | Trader |

### 3.3.3 Relationships used (and why)
- **Generalization (actor):** Market Creator ⊳ Trader — a creator can do everything a trader can, plus define and resolve markets, with one restriction: a creator cannot trade in their own markets. This avoids redrawing every trader association from the creator.
- **Include:** Create an account «includes» Receive the welcome gift (UC-1 → UC-3); Resolve automatically «includes» Validate the response against the options (UC-13); Submit a signed resolution «includes» Verify the ed25519 signature (UC-14); Settle and pay out «includes» Split the fee pot (UC-15 → UC-16); Place a trade «includes» Charge the 25 bps trading fee (UC-20). Each is mandatory sub-behavior, factored out rather than repeated.
- **Extend:** Void on missing or invalid evidence «extends» Settle and pay out (UC-17 → UC-15), because it runs only when no valid evidence arrives; Reject the creator's own trades «extends» Place a trade (UC-20), because it triggers only when the trader is the market's creator.

---

## 3.3.2 Use Case Descriptions

Full descriptions of all 25 use cases are in `docs/developer/use-cases.md`. The five below are the highest-value flows.

### UC-20 — Place a trade
- **Primary Actor:** Trader
- **Preconditions:** The user is signed in; an open market exists; the balance covers the trade.
- **Trigger:** The user chooses an outcome, a side and a share quantity on a market page.
- **Main Success Scenario:**
  1. The user previews the trade; the system returns a signed quote with the all-in amount, fee, average price and probability after the trade, valid for at most 15 seconds *(UC-18)*.
  2. The user confirms.
  3. The system re-checks the quote, account, market version, close time, balance, holdings and reserve inside one transaction.
  4. The system executes the trade atomically, charging the fee *(UC-20 includes the fee)* and updating balance, position and market inventory.
  5. The system returns a receipt and publishes a market event.
- **Extensions:**
  - 1a. The account is the market's creator → the quote is rejected; the use case ends.
  - 3a. Quote expired, market closed or suspended, balance insufficient, or limit not met → the trade is rejected and no units move.
  - 5a. The response is lost → the browser retries with the same idempotency key and receives the same receipt, or uses "Resume trade".
- **Postconditions:** One trade recorded; balances and prices updated.
- **Traces to:** FR-3.1 to FR-3.12

### UC-8 — Create a one-time market
- **Primary Actor:** Market Creator
- **Preconditions:** The user is signed in with the creator role.
- **Main Success Scenario:**
  1. The creator submits the title, resolution criterion, rule, evidence source, liquidity, fee choice and the close, observation, finalize and evidence times.
  2. The system validates the submission.
  3. The system publishes the series and its single instance, immutably.
  4. The system funds the instance's reserve from the treasury.
  5. The market appears in Browse markets *(UC-5)*.
- **Extensions:**
  - 2a. Not a creator → rejected (403). Invalid or duplicated outcomes, or inconsistent or past times → rejected; the creator edits and resubmits.
  - 4a. Treasury exhausted → the series is removed again and the submission is rejected.
- **Traces to:** FR-5.3 to FR-5.8

### UC-13 — Resolve automatically via the external resolver API
- **Primary Actor:** Settlement Worker; **Secondary Actor:** External Resolver
- **Preconditions:** The instance is closed and in its finalize window; its series has a resolver endpoint.
- **Main Success Scenario:**
  1. The worker posts the instance identifier, the bracket and observation window and the rule to the endpoint.
  2. The endpoint answers with exactly one published outcome identifier.
  3. The system validates the answer against the published outcomes and records it as evidence.
  4. Settlement proceeds *(UC-15)*.
- **Extensions:**
  - 2a. The endpoint is unreachable, answers pending, or returns a malformed answer → retried every tick until the evidence deadline.
  - 3a. Nothing valid by the deadline → the instance is voided *(UC-17)*.
- **Traces to:** FR-6.4, FR-6.5, FR-6.8

### UC-14 — Submit a signed human resolution
- **Primary Actor:** Market Creator
- **Preconditions:** A creator-owned market with human authority is closed; the creator can derive the signing key from the account password.
- **Main Success Scenario:**
  1. The creator selects the winning outcome.
  2. The browser signs the instance, outcome and a nonce with the private key.
  3. The server verifies the signature against the public key fixed at creation *(includes the signature check)*.
  4. The resolution is recorded and settlement proceeds.
- **Extensions:**
  - 3a. Invalid signature, submitter is not the creator, or market not closed → rejected.
  - 3b. An administrator attempts it → rejected by design; no valid signature exists.
  - 3c. The password is lost → the key cannot be re-derived; the market voids at its deadline.
- **Traces to:** FR-6.6, FR-6.7, NFR-5

### UC-5 — Browse markets
- **Primary Actor:** Trader (also available to a Visitor)
- **Preconditions:** None; discovery is public.
- **Main Success Scenario:**
  1. The user opens the markets page; the system lists live markets, soonest to close first, with category, state, top outcomes' probabilities and close time.
  2. The user narrows the list by category.
  3. The user opens a history tab (Closed, Voided or Resolved) to see past markets.
  4. The user opens a market card to reach its market page.
- **Extensions:**
  - 2a. No market on the page matches the category → an empty-state message is shown.
- **Traces to:** FR-2.1 to FR-2.6

---

## Traceability Matrix (requirement group → use case)

| Requirement group | Use Case(s) |
|---|---|
| FR-1 Account & Access | UC-1, UC-2, UC-3, UC-25 |
| FR-2 Market Browsing | UC-5, UC-18 |
| FR-3 Trading | UC-18, UC-19, UC-20, UC-21 |
| FR-4 Portfolio & Wallet | UC-4 |
| FR-5 Creator Lifecycle & Market Definition | UC-6, UC-7, UC-8, UC-9, UC-10, UC-11 |
| FR-6 Market Lifecycle & Resolution | UC-12, UC-13, UC-14, UC-15, UC-16, UC-17 |
| FR-7 Administration | UC-22, UC-23, UC-24 |

Test cases reference FR IDs directly (for example "TC-FR3.8-01: the creator's own trade is rejected") so traceability from requirement to test case to implemented feature is a direct lookup. The requirements-to-code matrix is in `docs/verification.md`.

---

## 3.4 UI Mockups — notes

The Lab 1 mockups (`Mock UI.png`) were low-fidelity wireframes drawn before the design settled, and they are kept as the original sketch. The built interface has these screens: markets and history, market page (probabilities, chart, trade preview and confirmation), portfolio, create-market form, and account dialogs including creator verification and the administrator view.

HCI principles applied in the built interface:
- **Visibility of system status:** live probabilities and the price history chart refresh with each market event, and a pending-trade notice appears when a trade response was interrupted.
- **Constraints / error prevention:** the trade preview form is disabled when a market is closed or suspended, and a signed-out user is told to sign in before previewing.
- **Error recovery:** "Resume trade" retries an interrupted trade once, using the same idempotency key.
- **Recognition over recall:** the market's resolution criterion is shown on its market page.

---

## Features in the earlier draft that are not built

The first draft of this document described GPS geofencing, "Confirm arrival" and queue-report submissions with photos and disputes, reputation scores, a leaderboard, password reset by email, and an administrator override of resolutions. None exists in the product. Resolution is by external resolver endpoint or by the creator's signature, and the administrator cannot resolve any market. "Queue/crowd" remains only as a market category.
