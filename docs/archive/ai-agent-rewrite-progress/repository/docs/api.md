PolyNTU API v2.

The default server address is `http://127.0.0.1:8000`. All routes below begin `/api/v2`. `GET /health` is outside that prefix and checks PostgreSQL connectivity. Request bodies are JSON, capped at 64 KiB. Business errors return `{"error":{"code":"...","message":"..."}}`; malformed extractor requests may return Axum validation text. Account routes use `Authorization: Bearer <account token>`. Administrator routes require `X-Admin-Token`.

| Method / path | Behavior |
|---|---|
| `GET /config` | Data mode, authoritative server time and unit scales. |
| `POST /auth/demo` | `{ "display_name": "Alex" }` creates a demo account with 1,000 units and returns its token once. Disabled outside demo mode. |
| `GET /me` | Current account and exact available balance. |
| `GET /markets` | Templates, categories and titles (up to 100). |
| `GET /instances?limit=100&offset=0` | Persisted occurrences, outcomes, probabilities, state and timing. |
| `GET /markets/{id}/instances` | Same instance list restricted to a template. |
| `GET /instances/{id}` | Detailed rules, current evidence, result and source metadata. |
| `GET /instances/{id}/events` | SSE `market` events. Resume using `Last-Event-ID` or `?after=<id>`, then fetch a snapshot. |
| `POST /quotes` | Account-bound price preview; no writes or inventory reservation. |
| `POST /trades` | Atomically execute a quote; requires `Idempotency-Key`. |
| `GET /me/portfolio` | Account, positions and one settlement credit per instance/account. |
| `GET /me/trades` | Private trade history. |
| `POST /admin/accounts` | Provision an account when public demo enrollment is disabled. |
| `POST /admin/instances` | Validate, fund and open an immutable instance. |
| `POST /admin/instances/{id}/evidence` | Append a source revision after the observation window. |
| `POST /admin/instances/{id}/suspension` | `{ "suspended": true, "reason": "Source investigation" }`. |
| `POST /admin/clock/advance` | `{ "minutes": 60 }`; demo mode only, 1–10,080 minutes. Runs a worker cycle. |
| `POST /admin/worker/tick` | Process due closing/evidence/settlement and recurring demo scheduling. |
| `GET /admin/reconcile` | Compare ledger balances, inventory, remaining claims and reserves. |

Instance/history list limits are clamped to 1–100 and offsets to 0–100,000. Portfolio positions, claims and history are paginated separately using the same offset. Historical positions retain their share count after redemption and include `settled: true`; credits are in the separate `settlements` list to avoid counting one account credit once per outcome.

A quote request:

```json
{
  "instance_id": "<instance UUID>",
  "outcome_id": "yes",
  "side": "buy",
  "quantity_millis": 10000
}
```

This previews 10 shares. The response includes `quote_token`, `amount_micros` (an integer string), `average_price`, before/after probabilities, `version`, `expires_ms`, and `server_time_ms`. A three-candidate election instead has outcome IDs `candidate-0`, `candidate-1`, `candidate-2`.

Execute the exact preview:

```http
POST /api/v2/trades
Authorization: Bearer <account token>
Idempotency-Key: <unique request UUID>
Content-Type: application/json

{"quote_token":"<signed quote>","limit_micros":"5124948"}
```

For buys, the limit is the maximum debit; for sales, it is the minimum credit. Use the quoted amount when the user approves exactly that amount. A successful receipt includes trade ID, balance, owned shares and new instance version. Reuse the same key and identical body after a network interruption or uncertain server error. A different body under a completed key returns 409. Expired or stale quotes return 409 and require a fresh preview. Inputs never specify their own account ID.

Create an instance using the `NewInstance` structure in `backend/src/market.rs`. Required fields: `template_id`, title, resolution criterion, typed rule, source ID, `data_mode` (`manual` or `simulated`), the five timing fields and `liquidity_units`. Outcome labels and inventory derive from the rule; callers cannot provide mismatched outcome sets. Creation opens the instance immediately after validation/funding. There is no draft-edit endpoint in v2.

Simulated instances require demo mode and source ID `polyntu-simulator-v1`. Manual evidence must arrive strictly before `evidence_deadline_ms`; at that deadline, an instance without complete evidence becomes eligible for voiding.

Weather evidence example (timestamps must exactly match the published window):

```json
{
  "source_id": "campus-weather-observer",
  "event_id": "station-reading-revision-2",
  "source_revision": 2,
  "window_start_ms": 1788832800000,
  "window_end_ms": 1788836400000,
  "observation": {
    "kind": "weather",
    "station_id": "station-S",
    "total_milli_mm": 400,
    "complete": true
  },
  "reference": "Archived source record identifier"
}
```

Rule fragments for the five categories (place one inside the instance's `rule` field):

| Market | Rule JSON |
|---|---|
| Rainfall | `{"kind":"weather","station_id":"station-S","threshold_milli_mm":200}` |
| Primary bus arrival | `{"kind":"bus","route_id":"campus-blue","direction":"clockwise","stop_id":"north-spine"}` |
| Fictional election | `{"kind":"election","candidates":["Avery","Blake","Casey"],"is_fictional":true}` |
| Queue length | `{"kind":"count","metric":"queue_length","location_id":"canteen-queue","threshold":20}` |
| Crowd occupancy | `{"kind":"count","metric":"crowd_occupancy","location_id":"library-zone-A","threshold":100}` |
| Event attendance | `{"kind":"count","metric":"unique_attendance","location_id":"event-session-001","threshold":150}` |
| Lecture attendance | `{"kind":"count","metric":"unique_attendance","location_id":"lecture-session-001","threshold":80}` |

For the bus rule, the question is whether an actual arrival occurs in the instance's fixed observation window. The observation contains matching route/direction/stop identifiers, `arrivals_ms` (an array of recorded arrival timestamps) and `complete` (coverage over the entire interval). There is no option strike, expiry payoff formula, or ETA-based settlement.

Count observations repeat `kind`, `metric` and `location_id` from the rule, and provide integer `value` plus `complete`. Attendance values count unique attendees supplied by the trusted source. Election observations contain `kind: "election"`, `winner` (an exact candidate label or `null`), `is_final`, and `is_fictional: true`. A final `null` winner applies the published void policy; non-final evidence waits.

When constructing times programmatically, start from `GET /config`'s `server_time_ms`, particularly in demo mode where the database clock can be advanced. Preserve the source ID and exact observation window in every evidence revision. Units, threshold and source semantics are part of the published contract and cannot be edited after opening.

The legacy `/contracts`, `/markets/{id}/quote` and `/markets/{id}/implied-sigma` routes return 410. Other retired routes are not mounted. No options pricing is reachable from the active application.
