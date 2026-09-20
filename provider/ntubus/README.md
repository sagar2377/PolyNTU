# NTU Bus API resolver provider

The live data source for PolyNTU's bus markets. The platform's bus adapter
(`POST /api/v2/resolvers/ntu-bus`, see the backend's resolver module) asks
this service first, over the external-resolver contract, and falls back to
the deterministic simulated feed only when this service stays without a
definitive answer. The evidence recorded on the platform carries a `source`
field showing which path answered (`ntubus-live` or `simulated-fallback`).

`ntubus_client.py` is the reverse-engineered NTU Omnibus API client (the
campus bus app's backend; anonymous session, no login). `service.py` is the
resolver: a background thread polls the live positions of every watched
route and each route's pickup points, and records an arrival event whenever
a bus comes within the stop radius. A request for a bracket window is
answered Yes when any arrival was recorded inside the window, No when the
poller watched the route through the whole window with no arrival, and
pending when it cannot know. When the Omnibus API itself stays unreachable
the service answers 503, so the platform falls back instead of recording a
false No.

## Run

```
pip install requests
python provider/ntubus/service.py
```

Then start the backend with the provider enabled (the default below points
at the service's default port):

```
$env:POLYNTU_NTUBUS_PROVIDER = "http://127.0.0.1:8090/resolve"   # default
cargo run --manifest-path backend/Cargo.toml --release
```

Set `POLYNTU_NTUBUS_PROVIDER` to an empty string to disable the live path
and always answer from the simulated feed (what CI does, since no provider
runs there).

## The contract the platform sends

```json
{
  "instance_id": "instance-uuid",
  "series_id": "series-uuid",
  "bracket_start_ms": 1789967400000,
  "window_start_ms": 1789967400000,
  "window_end_ms": 1789967520000,
  "outcomes": [{"id": "yes", "label": "Yes"}, {"id": "no", "label": "No"}],
  "title": "Blue line · arrival at Opp SPMS · 15:50 to 15:52",
  "evidence_deadline_ms": 1789967580000,
  "rule": {"kind": "bus", "route_id": "NTU-blue", "direction": "clockwise", "stop_id": "opp-spms"}
}
```

`outcomes` carries every published outcome id and label, so an integrator
can answer without a second lookup; `evidence_deadline_ms` is the moment
after which answers no longer count. The answer must be exactly one of
`{"outcome_id": "<published id>"}` or `{"pending": true}`; extra fields
(like this service's `reason`) are ignored by the platform.

Route ids arrive with the platform's prefixes (`NTU-blue` for campus lines,
`SBS-179` for public ones); the service strips them and matches the Omnibus
route codes case-insensitively. Stop ids are kebab-case pickup point names
(`opp-spms` matches "Opp SPMS"). Campus loop routes each run one direction,
so `direction` is informational. Any route the client can serve can be
watched; the default covers all five campus lines, including Grey (which
has no published schedule and is not seeded as a demo market).

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `NTUBUS_BIND` | `127.0.0.1:8090` | Listen address |
| `NTUBUS_ROUTES` | `Blue,Red,Green,Brown,Grey` | Comma-separated Omnibus route codes to watch |
| `NTUBUS_POLL_SECONDS` | `12` | Poll cycle for live positions |
| `NTUBUS_RADIUS_M` | `60` | A bus within this many metres of a pickup point counts as arriving |
| `NTUBUS_BUFFER_MINUTES` | `120` | How long arrival events and poll coverage are kept |
| `NTUBUS_GEOMETRY_MINUTES` | `10` | Pickup point refresh interval |
| `NTUBUS_CLIENT_ATTEMPTS` | `3` | Retries per Omnibus API call |

## Notes and limits

- The poller runs on wall-clock time. The demo database clock can run ahead
  (administrator advances); windows in the demo future have no coverage, so
  the service answers pending and the platform's simulated fallback keeps
  the demo settling.
- Arrival detection is positional proximity, not the app's ETA; a bus
  dwelling at a stop produces one event per poll, which still answers the
  binary question correctly.
- Coverage requires polls before and throughout the window with no gap over
  2.5 poll cycles; restarting the service mid-window answers pending for
  that window rather than guessing.
