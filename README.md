# ShuttlePredict

**Purpose:** ShuttlePredict is an NTU SC2006 software engineering project whose
goal is to **improve the accuracy and communication of NTU shuttle bus
arrival predictions**. Instead of showing riders a single point estimate
("arriving in 6 min"), it prices the *uncertainty* around that estimate —
borrowing the mathematical machinery of derivatives pricing (Black-Scholes,
the Greeks, Monte Carlo simulation) to turn "how confident should I be that
the bus arrives within X minutes?" into a quoted, auditable number.

**This is not a gambling or betting application, and it involves no real
money.** There is deliberately no betting/wagering/odds/payout vocabulary
anywhere in this codebase. All prices are labeled **simulated confidence
units** — an internal, dimensionless quantity produced by the pricing model,
never a currency. The finance terminology used (contract, position, pricing
model, settlement, in-the-money/out-of-the-money) is used in its academic,
descriptive sense, the same way a finance textbook uses it to explain a
model — not as a product inviting a wager.

## The idea

A shuttle's arrival time is uncertain right up until it arrives. That
uncertainty is exactly the kind of thing derivatives pricing theory was
built to quantify: an option's value depends on how *likely* an uncertain
future price is to end up on one side of a threshold. Swap "stock price" for
"predicted arrival delay" and "strike price" for "a threshold arrival
window," and the same closed-form math (and the same Greeks, for the same
reasons) applies to "how likely is this bus to be on time?"

## Domain mapping

| Options concept | ShuttlePredict equivalent |
|---|---|
| Spot price (S) | Current predicted arrival delay (minutes), transformed — see below |
| Strike price (K) | A threshold delay window being evaluated (e.g. "within 5 min"), same transform |
| Time to expiry (T) | Hours remaining until the bus's scheduled arrival |
| Volatility (σ) | Historical variance in arrival delay for that route/hour, calibrated from simulated/logged data |
| Risk-free rate (r) | Fixed at 0 — not meaningful in this domain |
| "Call" | Confidence contract: bus arrives **before** the threshold (delay < threshold) |
| "Put" | Confidence contract: bus arrives **after** the threshold (delay > threshold) |
| Implied volatility solver | Inverts a quoted confidence price to back out "implied punctuality," comparable against the route's historically-calibrated volatility |

### Why "Call"/"Put" need a sign transform

Black-Scholes requires a strictly positive, lognormal underlying. Arrival
delay isn't positive — a bus can be early. `backend/app/pricing.py` shifts
and negates the delay (`underlying = -delay + SHIFT_MINUTES`) before handing
it to the untouched Black-Scholes formulas, the same trick real rates desks
used for "shifted Black" models once interest rates went negative. The
negation is what makes "Call" line up with "arrives before threshold" — see
the module docstring in `pricing.py` for the full derivation.

## Known modeling limitation (by design, documented in code)

Bus arrivals are **bounded and schedule-anchored**, not a free random walk:
buses rarely arrive much earlier than scheduled, and delay is right-skewed
(mostly on-time, occasionally very late, almost never very early). GBM — the
process Black-Scholes and its Monte Carlo cross-check assume — is unbounded
and symmetric in log-space, so it doesn't fully match reality here.

`backend/app/simulator.py` generates the underlying arrival data from a
shifted Gamma distribution (bounded, right-skewed) instead of GBM, and
`pricing.monte_carlo_bootstrap_price` cross-checks the analytical price
against a bootstrap resample of that realistic data rather than a GBM
simulation. The gap between the analytical/GBM price and the bootstrap price
*is* the model risk of applying Black-Scholes' lognormal assumption to a
process that isn't lognormal — the dashboard surfaces this gap rather than
hiding it, and `test_pricing.py` has an explicit test asserting the gap
exists (`test_bootstrap_monte_carlo_diverges_from_gbm_when_distribution_is_skewed`).

## Architecture

- **`backend/`** — FastAPI + SQLite. Reuses the existing Black-Scholes
  engine (`app/bs_engine.py`, adapted from a prior options-pricing project)
  almost verbatim; `app/pricing.py` is the domain-mapping layer on top of
  it; `app/simulator.py` is the data layer (no public NTU shuttle API
  exists, so this stands in for one); `app/settlement.py` is the contract
  state machine (`CREATED → ARRIVED → SETTLED`, or `→ EXPIRED`); `app/db.py`
  persists contracts and the historical delay log to SQLite; `app/main.py`
  wires it all into an HTTP API.
- **`frontend/`** — React (Vite). A dashboard showing live predicted
  arrivals, priced confidence contracts per threshold with a Greeks
  breakdown, and a position blotter with settlement outcomes.

## Running it

```bash
# backend
cd backend
python3 -m venv venv && source venv/bin/activate
pip install -r requirements.txt
uvicorn app.main:app --reload --port 8000

# frontend (separate terminal)
cd frontend
npm install
npm run dev
```

Open the printed Vite URL (typically `http://localhost:5173`). The frontend
talks to the backend at `http://localhost:8000` by default (override with
`VITE_API_URL`).

## Testing

```bash
cd backend && source venv/bin/activate
python -m pytest tests/ -v
```

33 tests cover: the reused Black-Scholes engine against the original
project's known-good sanity-check numbers, the pricing domain layer
(including the intentional GBM-vs-bootstrap divergence), the settlement
state machine's legal/illegal transitions, the SQLite persistence layer, and
the FastAPI endpoints end-to-end (create a contract → advance the simulated
clock → auto-settlement).
