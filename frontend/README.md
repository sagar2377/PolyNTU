# PolyNTU React frontend

The frontend is a React 19 single-page interface built with Vite. It displays public markets, manages a locally stored demo account token, previews and confirms trades, recovers uncertain receipts, and displays positions and settlement credits. All authoritative trading and resolution decisions remain in the Rust backend.

## Run the frontend separately

```powershell
npm ci
npm run dev
```

Vite proxies `/api` and `/health` to `http://127.0.0.1:8000`. Set `VITE_API_URL` only when the API is hosted at another origin. `npm run build` creates `dist/`, and `npm run lint` checks the source.

## Source map

| File | Responsibility |
|---|---|
| `src/App.jsx` | Account access, navigation, global refresh, errors, pending-trade notice, and demo clock controls. |
| `src/api.js` | Fetch wrapper, bearer/admin headers, exact unit formatting, share parsing, timestamps, and storage keys. |
| `src/pages/MarketBrowse.jsx` | Paginated market discovery and category filtering. |
| `src/pages/MarketPage.jsx` | Instance snapshot, event subscription, resolution evidence, and trade panel composition. |
| `src/pages/Portfolio.jsx` | Positions, settlement credits, and trade history. |
| `src/components/TradePanel.jsx` | Quote preview, expiry countdown, confirmation, idempotency persistence, and receipt recovery. |
| `src/App.css` | Application layout, components, responsive rules, and focus styles. |

## Browser storage

- `polyntu.v2.token` stores the current bearer token.
- `polyntu.v2.pending-trade` stores an exact submitted trade body, its account and instance IDs, and its idempotency key until a definitive response is received.

Local storage is acceptable for the present loopback demonstration but is not sufficient production credential handling. See [frontend internals](../docs/developer/frontend.md) and [security and privacy](../docs/developer/security-and-privacy.md).
