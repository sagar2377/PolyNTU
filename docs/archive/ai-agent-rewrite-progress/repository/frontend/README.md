PolyNTU React frontend.

Run `npm ci` and `npm run dev` here to start Vite. The development proxy sends `/api` and `/health` to the Rust backend at `127.0.0.1:8000`. `npm run build` creates `dist/`, which the Rust service serves directly. `npm run lint` checks the source.

`App.jsx` manages account access and navigation. `MarketBrowse` groups instances across five categories, `MarketPage` displays immutable rules and subscribes to market events, `TradePanel` previews and confirms outcome-share purchases/sales, and `Portfolio` shows the signed-in account's positions and trades.

`api.js` owns API calls and unit conversion. Credit amounts stay integer strings and format through `BigInt`; shares parse to millishares without floating-point arithmetic. Quote expiry is displayed as a countdown, while the backend always enforces the authoritative cutoff. Changed market versions require another preview.

Before sending a trade, the client saves its exact body and idempotency key in browser storage. An uncertain connection error retains that request so retrying retrieves the same receipt. Account tokens are also stored locally for this demo. Sign out removes the token; retain a private copy through Account access if you need to restore the account. Institutional authentication and token recovery are future work.

Market events trigger snapshot refreshes, with periodic polling as a reconnect fallback. All trading and resolution logic lives in Rust; the frontend only displays prices and submits requests. See [API](../docs/api.md), [architecture](../docs/architecture.md), and [development](../docs/development.md).
