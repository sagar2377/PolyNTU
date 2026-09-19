# Frontend code reference

The React client is intentionally thin. It renders server snapshots, performs user-friendly input checks, and preserves uncertain trade requests. Backend time, prices, ownership, balances, state, evidence, and settlement remain authoritative.

## Build and entry point

`frontend/src/main.jsx` mounts `<App />` into `#root` under React `StrictMode` and loads `index.css`. `vite.config.js` installs the React plugin and proxies `/api` and `/health` to loopback port 8000 during development.

`index.css` contains only global box sizing/body margin. Most component and responsive styling lives in `App.css`.

## API client: `src/api.js`

### Request wrapper

`request(path, options)`:

1. chooses `VITE_API_URL` or same origin;
2. sets JSON content type;
3. uses an explicitly supplied token or the stored account token;
4. adds administrator/idempotency headers when provided;
5. JSON-serializes defined bodies;
6. translates fetch/response-read interruptions into receipt-retry guidance;
7. parses JSON; and
8. throws `ApiError(message,status)` for non-2xx responses.

The wrapper always calls `/api/v2`; health is served separately and is not used by the current browser UI.

### Endpoint helpers

The exported `api` object wraps configuration, account lookup, demo account creation, NTU registration, email/password login, creator verification submission/lookup, instance lists/detail, quotes/trades, portfolio/history, demo clock advance, and event URL construction. It does not currently expose template lists, administrator instance/evidence/suspension creation, worker tick, or reconciliation.

### Exact unit functions

- `units(micros)` converts the input to `BigInt`, separates sign, formats whole units with `en-SG`, trims trailing fractional zeroes, and never uses floating-point accounting.
- `quantityMillis(value)` accepts unsigned decimal text with at most three fractional digits, converts it to 1–100,000 millishares, and rejects unsafe JavaScript integers.
- `timestamp(ms)` formats Asia/Singapore year/month/day/hour/minute.

Probabilities and average prices are display `Number` values supplied by the backend. They must not be reused to construct ledger amounts.

### Storage keys

| Key | Stored value |
|---|---|
| `polyntu.v2.token` | Current plaintext account bearer token |
| `polyntu.v2.pending-trade` | Account/instance IDs, exact trade body, and idempotency key |

`pendingTrade` parses the second key defensively and returns null on malformed JSON.

## Application shell: `src/App.jsx`

### State

`App` tracks server configuration, authenticated account, current view, selected instance, refresh counter, global error, registration/login form inputs, demo account name and access-token inputs, administrator token input, the current verification request, and a shared busy flag.

Views are string-selected rather than URL-routed:

- `browse` renders `MarketBrowse`;
- `market` renders the selected `MarketPage`; and
- `portfolio` renders `Portfolio`.

Refreshing the browser does not preserve the selected view/market, but the account and pending trade remain in local storage.

### Account entry panel

Before authentication, the entry panel leads with the NTU registration form (display name, NTU email, password of at least 12 characters, validated client-side to the server's rules). A collapsible email and password login form follows. In demo mode, further disclosures add the one-click demo participant and a sign-in button for the seeded administrator (`admin@ntu.edu.sg`); the access-token paste flow remains the last disclosure. Once authenticated, the header account chip shows the display name, the role when present, and the available units.

### Creator verification panel

Members (`account.role === "member"`) see a verification panel with three states: request creator verification, a pending-review notice, or the rejection reason with an apply-again button. `App` fetches the account's own latest request whenever the account identity or role changes and keeps it in state; sign-out clears it.

### Polling and account restoration

An effect requests configuration and, when a token exists, `/me`. It repeats every five seconds and reruns after the refresh counter changes. Cleanup marks the effect cancelled and clears its timer so stale promises cannot update state.

Registration, login, demo account creation, and token sign-in all store the returned token and account; token sign-in verifies `/me` first. A 401 from the polled `/me`, meaning the token was rotated out by a login elsewhere, clears the stored session and account instead of erroring on every poll. Sign-out deletes the account token and clears the verification state; a pending trade is intentionally preserved so it cannot be silently lost.

### Pending trade notice

The app reads pending storage each render:

- matching current account: offer “Resume trade” and open the recorded instance;
- different/missing account: explain that the saved request belongs to another account.

The actual retry occurs inside `TradePanel`.

### Demo administration

When configuration says demo mode, a disclosure accepts the administrator token in component state and can advance one hour or one day. The token is not persisted by the frontend. A successful advance increments the refresh counter.

This control is local-demo convenience, not an administrator console.

## Market discovery: `pages/MarketBrowse.jsx`

State contains the current page of instances, selected category, loading flag, and offset. The page loads immediately and every ten seconds. Category filtering is client-side over only the current 100-row page.

Each card shows category, effective state, up to three outcomes, marginal percentages, data-mode label, and Singapore close time. Pagination increments by 100 and disables Next when fewer than 100 rows arrive.

Consequences:

- “open on this page” is not a platform-wide count;
- category filtering does not fetch all pages for that category; and
- templates are not displayed separately.

## Instance page: `pages/MarketPage.jsx`

The page loads the instance immediately, opens a public `EventSource`, reloads after a 100 ms debounce on each `market` event, and also polls every five seconds. Cleanup closes the stream and timers.

It renders:

- final/resolving result banner;
- all marginal probabilities;
- published times/liquidity;
- resolution criterion, source, and void policy;
- selected public evidence payload inside a disclosure; and
- `TradePanel`.

The event URL does not include a cursor explicitly; native EventSource reconnects can supply `Last-Event-ID`, while periodic snapshots cover missed/gapped updates.

## Trading UI: `components/TradePanel.jsx`

### Input and quote identity

The panel tracks outcome, side, share text, quote, busy/error/receipt, matching pending request, expiry countdown, request sequence, and an execution ref.

`requestKey = outcome:side:quantity:instance.version` prevents a quote from rendering after the visible request inputs or instance version change. `sequence` prevents an older asynchronous preview response from replacing a newer request.

### Preview

On submit, it converts the quantity, sends the four-field quote request, and stores:

```text
local_deadline = local Date.now()
               + response.expires_ms
               - response.server_time_ms
```

This uses the server's reported remaining lifetime while allowing the browser to count down locally. The backend still enforces actual expiry and close.

### Confirmation and recovery

Before sending a new trade, the panel constructs:

- current account ID;
- instance ID;
- exact `{quote_token,limit_micros}` body;
- a new `crypto.randomUUID()` idempotency key.

It writes that object to local storage before calling the API. A successful response removes storage and displays the receipt. A definitive 400, 404, 409, or 422 removes the pending record and stale quote. A connection/response/500-class failure retains it.

`executing.current` supplements `busy` to prevent rapid duplicate submissions before React state updates. When a pending request exists, the panel hides normal trade entry and offers exact retry/retrieval.

Only one global pending trade is supported per browser profile. Another market/account is blocked until it is resolved.

### Account display staleness

The panel receives the account snapshot owned by `App`. After a successful trade it increments the global refresh counter; the account balance then refreshes asynchronously. The receipt immediately displays its authoritative post-trade balance.

## Portfolio: `pages/Portfolio.jsx`

When an account exists, the page requests portfolio and trade history concurrently on load and every five seconds. One offset is sent to all three logical lists: positions, settlement credits, and trades.

It renders:

- current available balance;
- positive historical/current outcome positions;
- one settlement credit per instance; and
- private trade history.

Next is disabled only when all three returned lists contain fewer than 100 entries. Because each list is paginated independently using one offset, pages can be sparse for one section and full for another.

## Styling and accessibility

`App.css` provides responsive grids, semantic focus-visible outlines, muted/status/error colours, horizontal table overflow, a screen-reader-only utility, and a single-column layout below 760 px.

Implemented semantic aids include navigation labels, form labels, fieldsets/legends, `aria-pressed`, alert/status roles, and keyboard-focus outlines. Visual contrast, screen-reader flow, zoom, mobile interaction, and browser compatibility still require human review; project instructions prohibit automated browser control.

## Security limitations

- The bearer token is plaintext in local storage and readable by same-origin JavaScript.
- No Content Security Policy is defined in this repository.
- The demo UI accepts a powerful administrator token in memory.
- Sign-out does not revoke a token at the backend; logging in again does rotate it, which signs out every other browser on the next poll.
- Error text is displayed to the user; backend 500 messages deliberately hide details.
- Public evidence JSON is displayed verbatim and must already be non-sensitive.

These are acceptable only within the stated loopback academic-demo scope. See [security and privacy](security-and-privacy.md).

## Frontend change checklist

For API or state-flow changes, update `api.js`, affected components, exact-unit handling, pending-request behaviour, API docs, frontend tests if introduced, lint/build checks, accessibility review, and the traceability matrix.

