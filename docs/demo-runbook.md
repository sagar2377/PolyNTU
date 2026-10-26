# PolyNTU demonstration runbook

A single scripted walkthrough of the whole product, in the order it should be
presented. Every step names what the audience should see and how to recover if
it does not appear. The runbook assumes the two-server setup described below,
which lets the demonstration switch from simulated to live evidence without
restarting anything.

The demo is a **tour of implemented behaviour**, not a proposal: every step
below runs against the current build. Where a step touches a limitation, it
says so rather than hiding it.

## Why two servers

| Instance | Port | Database | Clock | Bus evidence |
|---|---:|---|---|---|
| Simulated | `http://127.0.0.1:8000` | `polyntu` | Virtual, advanceable | Deterministic simulator (`POLYNTU_NTUBUS_PROVIDER=""`) |
| Live | `http://127.0.0.1:8001` | `polyntu_live_demo` | Wall clock, not advanceable | Campus Omnibus feed through `provider/ntubus` |

They must use **separate databases**. Advancing the demo clock on the simulated
instance moves every new bracket into the provider's wall-clock future, which
makes the live feed answer `pending` forever; the two modes cannot share one
database's clock. The live instance refuses `POST /admin/clock/advance`
outright, and the interface hides both the Demo clock and the Admin buttons
there, because neither operation can succeed.

## Before the audience arrives

1. Start PostgreSQL and the bus provider: `./scripts/start-local-db.ps1`, then
   `python service.py` from `provider/ntubus/`.
2. Start both instances, each in its own terminal:
   - `./scripts/run-sim.ps1` (simulated, port 8000)
   - `./scripts/run-live-demo.ps1` (live, port 8001; creates its own database
     on first run)
3. Confirm both are ready: `curl http://127.0.0.1:8000/health` and
   `curl http://127.0.0.1:8001/health` both return `status: ok`, and
   `curl http://127.0.0.1:8090/` reports `"ok": true` with non-zero polls.
4. Open both URLs in separate browser windows, or in two tabs of one window.
   Keep the simulated one in front; the live one is used only in step 9.
5. Let the simulated instance run for a few minutes before starting. The demo
   worker adds one fill per seeded market every 45 seconds, so the charts and
   the browse grid already show movement instead of a flat 50/50 line. The
   first fills land within a minute of start-up; a market with only one fill
   still reads close to 50/50, so give it three or four minutes.

### The three account types

| Account | Credentials | What it demonstrates |
|---|---|---|
| **Member** (registered) | Any `@ntu.edu.sg` address, password ≥ 12 characters | Registration, welcome gift, verification request, trading, portfolio |
| **Creator** | `bus@ntu.edu.sg` / `bus`, or a member just approved | Publishing a series, signing a resolution, fee share |
| **Administrator** | `admin@ntu.edu.sg` / `admin`, or the token in `.local/dev-secrets.json` | Verification queue, provisioning, settlement tick, reconciliation, demo clock |

A one-click **demo account** (1,000 units, no email or password) also exists in
demo mode. It can browse and trade but cannot file a verification request or
publish, because it holds no role.

## The walkthrough

### 1. Browse the market grid (visitor)

Open `http://127.0.0.1:8000` without signing in.

- The grid lists live markets, soonest to close first: the four seeded bus
  lines and five one-time demo markets, each with its outcome probabilities.
- The probabilities are **not** all 50/50: the demo participants have traded
  each market, so the split has moved.
- The category filter narrows the grid; each card carries a data-mode label
  ("Simulated observations") and its close time in Singapore time.
- History is collapsed: the Closed, Voided, and Resolved tabs are hidden until
  clicked.

### 2. Register as a member and trade

1. **Create account** (top right): display name, an `@ntu.edu.sg` address, and
   a password of at least 12 characters.
2. The header now shows the display name, the `member` role, and a
   10,000-unit balance.
3. Open **Rainfall · campus weather station**.
4. In the trade panel, choose an outcome and quantity, then **Preview trade**.
   A signed quote appears with its all-in amount, the 0.25% fee, the
   probability after the trade, and a live countdown to expiry.
5. **Confirm buy**. The receipt shows the shares, the units debited, the new
   balance, and a trade ID.
6. Point out the **price and volume history** chart: it now carries the demo
   participants' earlier fills as well as this one, so it is a stepped line
   with volume bars, not a flat line.

The quote is an HMAC-signed token bound to this account, market, outcome, side,
quantity, amount, fee, market version, and a 15-second expiry. The trade is
idempotent: the same idempotency key returns the same receipt instead of
trading twice.

### 3. File a creator verification request

On the same member account, the panel **Become a market creator** offers
**Request creator verification**. Click it; the panel switches to "pending
administrator review". A second request is rejected while one is pending.

### 4. Approve the request as the administrator

1. Sign out, then **Log in** as `admin@ntu.edu.sg` / `admin`.
2. Open the **Admin** button in the footer. (In live mode this button is not
   shown, because none of the console's operations can run there.)
3. Under **Creator verification queue**, click **Load pending requests**, then
   **Approve** on the member's request.
4. Sign back in as the member: the header now shows the `creator` role and the
   **Create market** navigation button has appeared.

The decision is recorded in the append-only audit trail, and approval only
updates an account that still holds `role='member'`, so a stale request cannot
promote an account that changed in the meantime.

### 5. Publish a creator-signed market

As the newly approved creator:

1. **Create market**.
2. Fill in a title, a specific resolution criterion, and choose a category.
3. Set **Resolution mode** to *Manual update by the market creator*. This fixes
   creator authority at publication.
4. Because the signing key is cached from sign-in, no password prompt appears.
   (Sign out and back in, or clear `polyntu.v2.signing-key`, to demonstrate the
   password prompt: the key is re-derived from the password and never leaves
   the browser.)
5. Choose a **One-time market** schedule, a close time a few minutes ahead, and
   an observation window of a few minutes.
6. **Publish market**. The new market's page opens.

The published definition fixes the creator's ed25519 public key, and the
database trigger makes it immutable.

### 6. Trade the creator's market from the member account

Sign back in as the member and trade the new market, so it has a position and a
price history before it closes. The creator cannot trade it: attempting to do
so from the creator account is rejected with "Creators cannot trade in their
own markets".

### 7. Close and resolve with the creator's signature

1. As the administrator, open **Demo clock**. Enter the administrator token,
   then use the minute field to advance to just past the market's observation
   end. The presets (1 hour, 1 day) jump clean over the resolvable window: a
   creator market is resolvable only between its observation end and its
   evidence deadline, which is 60 seconds wide. Advancing by an exact number of
   minutes is what lands inside it.
2. The worker runs a settlement pass on the advance, so the market closes
   immediately.
3. Sign in as the creator and open the market.
4. Under **Bracket history**, open the **Closed** tab. The bracket is listed
   with an outcome selector and a **Resolve** button.
5. Pick the winning outcome and click **Resolve**.
6. The market settles: the **Resolved** tab shows the result, and the member's
   portfolio shows a settlement credit.

What the audience should understand: the platform never held the private key.
The browser signed the exact string
`polyntu.resolution.v1:{instance_id}:{outcome_id}:{nonce}` with the key derived
from the password, and the server verified it against the public key fixed at
publication. A compromised administrator cannot resolve this market: there is
no administrator evidence route at all.

### 8. Inspect the portfolio, the fee split, and reconciliation

- **Portfolio** (member): outcome shares with their state, settlement credits,
  and the trade history.
- The creator's balance has grown by the creator's half of the 25 bps fee pot;
  the treasury received the other half and the unused reserve.
- As the administrator, **Admin → Reconcile** reports no discrepancies: balances
  match the ledger, inventory matches positions, reserves cover obligations,
  and the sum of all balances is zero because issuance is the only negative
  account.

### 9. Switch to the live instance: the external connector

Switch to the live window at `http://127.0.0.1:8001`. Everything is the same
product; only the bus evidence source differs.

1. The footer no longer offers **Demo clock** or **Admin**: live mode refuses
   clock advances and the console's operations cannot run there.
2. The one-time demo markets are **not seeded in live mode**, because their
   evidence would be simulated and live mode refuses to fabricate an answer, so
   they would only void.
3. The seeded bus series run 07:30–23:00 SGT. **Outside those hours there is no
   bracket to show**, so publish one: sign up, get verified, and publish a bus
   series with **Resolution mode: Automatic resolution via a provided API** and
   the endpoint `http://127.0.0.1:8001/api/v2/resolvers/ntu-bus`. The adapter
   accepts a creator-published bus market when its authority is the external
   resolver, so its brackets answer from the live feed like the seeded ones.
   The automated demo script does exactly this when the seeded lines are out of
   hours.
4. Open a bus bracket and scroll to **How this market resolves**: the authority
   is the external resolver, shown as **NTU Bus API**.
5. Wait for the bracket's two-minute observation window to end. The settlement
   worker POSTs the fixed resolver request to the platform's own adapter, which
   asks the Python provider, which answers from the campus Omnibus feed.
6. The evidence disclosure on the market page shows `"source": "ntubus-live"`
   and the recorded request and response. The label reads **Live observations**.
7. If the provider cannot answer — no coverage, a restart mid-window, the
   Omnibus API unreachable — the bracket records nothing and **voids at its
   published deadline**. It never fabricates an outcome. This is the visible
   difference from the simulated instance.

The provider answers Yes when a bus came within 60 m of the stop during the
window, No when it watched the whole window with no arrival, and pending when
it cannot know.

### 10. Point at what is deliberately absent

Worth saying explicitly, because the demo is more credible for it:

- No campus SSO, password recovery, rate limiting, or production deployment
  hardening.
- Resolution trusts the key holder or the configured endpoint; the platform
  cannot independently prove a real-world observation.
- Recurring-series brackets and their trades, evidence, and credits are purged
  24 hours after their evidence deadline; one-time markets and the ledger are
  kept forever.
- The shared administrator token carries no per-action identity.

## Recovery

| Symptom | Cause | Fix |
|---|---|---|
| Demo trades never appear | Worker not ticking, or the interval has not elapsed | Check `/health`, wait 45 s, inspect the server log |
| Live bracket voided | Provider had no coverage for the window | Confirm `curl http://127.0.0.1:8090/` reports `"ok": true`; use a fresh database or wait for the next bracket |
| Live grid is empty outside 07:30–23:00 SGT | The seeded bus series are outside their operating hours | Publish a resolver-authority bus series covering the current hour (step 9) |
| Clock advance refused on 8001 | Live mode forbids it | Expected; advance only on the simulated instance |
| Market voids instead of resolving | The clock jumped past the 60-second resolvable window | Advance by exact minutes, not the 1-hour or 1-day presets |
| Verification panel missing | Signed in as a demo account or a creator | Only `member` accounts file requests |
| "That password does not match the key published for this series" | Wrong password, or a different account | Sign in as the series creator |
| Resolution rejected with 409 | Window not open, deadline passed, or already resolved | One resolution per instance; check the bracket's timestamps |

## This runbook is operated by a person

The walkthrough is presented live: a person signs in, clicks, and narrates. It
is deliberately not automated, because the point is to show the audience what
the product does, not to replay a recording.

The automated equivalent is `scripts/browser-e2e.mjs`, which runs in CI on
every backend, frontend, or test-script change. It drives this whole runbook
over the Chrome DevTools protocol — both account types, the seeded demo price
history, registration, trading, the verification workflow through the
administrator console, publishing a creator-authority market, the self-trading
ban, the signed resolution, the settlement credit, reconciliation, the browse
collapse, both password prompts, and the live instance settling from the
provider — and fails the build if any of it regresses. That is where automated
coverage belongs; this document is the human script.

To check the demo still works before presenting it, run the CI test locally.
It needs both instances and a bus provider; `scripts/ntubus-stub.mjs` is a
deterministic stand-in that answers the same contract as the real provider, so
the check does not depend on the campus feed:

```text
node scripts/ntubus-stub.mjs 8090
node scripts/browser-e2e.mjs
```

Set `E2E_SKIP_LIVE=1` to run only the simulated half.

## Related documents

- [API contract](api.md): the creator resolution and external resolver contracts.
- [Evidence and settlement](developer/evidence-and-settlement.md): how each authority records evidence.
- [Security and privacy](developer/security-and-privacy.md): key custody and the administrator exclusion.
- [ADR 0007](decisions/0007-resolution-authority.md): why the platform never holds a signing key.
- [Operations](developer/operations.md): modes, health checks, and troubleshooting.
