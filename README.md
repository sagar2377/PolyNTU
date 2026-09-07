# PolyNTU

**Purpose:** PolyNTU is an NTU SC2006 software engineering project — an
**academic prediction-market platform** for the NTU campus. It prices
"confidence contracts" on uncertain campus outcomes, borrowing the
mathematical machinery of derivatives pricing (Black-Scholes, the Greeks,
Monte Carlo simulation) to turn "how confident should I be that X happens?"
into a quoted, auditable number. This is the same category of platform as
Metaculus or the academic framing of Polymarket/Kalshi — a forecasting and
communication tool, not a wagering product.

**This is not a gambling or betting application, and it involves no real
money.** There is deliberately no betting/wagering/odds/payout vocabulary
anywhere in this codebase. All prices are labeled **simulated confidence
units** — an internal, dimensionless quantity produced by the pricing
model, never a currency. Finance terminology (contract, position, pricing
model, settlement, resolution, in-the-money/out-of-the-money) is used in
its academic, descriptive sense — the same way a finance textbook uses it
to explain a model.

**On the student-election market type specifically:** the only election
market this project ships uses a **fictional** demo race (placeholder
candidates "Candidate A"/"Candidate B", a made-up "Demo Hall Committee
Election," simulated polling data). Building a market that prices a real,
in-progress election's real candidates was deliberately declined — even
with simulated currency, it would expose real people to a public
"confidence score" without consent and risks influencing the very vote it
claims to just forecast, the same dynamic that's drawn regulatory scrutiny
for Polymarket/Kalshi's real-election contracts. `market_types/
student_election/market_type.py`'s `validate_params` hard-requires
`is_fictional: true`, so a real election can't be configured by accident.

PolyNTU generalizes **ShuttlePredict**, this project's original single-market
prototype (arrival-window confidence contracts for the NTU shuttle), into a
platform with a market-agnostic core and pluggable **Market Types**.
ShuttlePredict is Market Type #1; a fictional student-election market is
Market Type #2, added specifically to prove the abstraction generalizes to
a market with the opposite sign convention and a single multi-day
resolution event instead of many short recurring ones.

## Every market publishes a resolution criterion

Every market states, up front and in plain language, exactly how and
against what data source it settles (see the `resolution_criterion` field
returned by `GET /markets`, and the demo markets' criteria in
`backend/app/main.py`'s `DEFAULT_MARKETS`). `POST /admin/markets` rejects
any market creation request with an empty or placeholder ("TBD") criterion
— this is both good prediction-market practice and something the
`test_admin_create_market_rejects_placeholder_resolution_criterion` test
enforces.

## Architecture

```
backend/
  core/            market-agnostic engine — never imports a concrete MarketType
    bs_engine.py     Black-Scholes / Greeks / Monte Carlo math (pure, reused unchanged)
    calibration.py   absolute stdev -> fractional BS volatility
    live_estimate.py a live estimate that converges to a true value as resolution nears
    pricing.py       generic threshold-contract pricing, parameterized by a MarketType
    market.py        Market / MarketInstance / MarketType abstraction + registry
    settlement.py    generic OPEN -> RESOLVING -> SETTLED / EXPIRED state machine
    db.py            SQLite persistence (markets, contracts, historical observations)

  market_types/    plugins — each supplies simulation + calibration + resolution only
    shuttle_arrival/    Market Type #1 — shuttle arrival delay
    student_election/   Market Type #2 — fictional vote-share demo

  app/             thin HTTP + orchestration layer (FastAPI)
    main.py          /markets, /markets/{id}/instances, /markets/{id}/quote,
                      /contracts, /clock/advance, /portfolio, /admin/markets
    state.py         in-memory simulated clock + generated MarketInstances

frontend/src/
  pages/           MarketBrowse (homepage), MarketPage (per-market), Portfolio
  components/      InstanceList, QuotePanel, Greeks, ContractsBlotter — all
                    market-agnostic, driven entirely by what the API returns
```

See `docs/adding-a-market-type.md` for the checklist to add a new one.

## Domain mapping (generic, see each market type's module docstring for specifics)

| Options concept | PolyNTU equivalent |
|---|---|
| Spot price (S) | A market's current live-estimated domain value |
| Strike price (K) | A threshold being evaluated for that value |
| Time to expiry (T) | Time remaining until the market's resolution event |
| Volatility (σ) | Historical variance of the domain value, calibrated per market |
| Risk-free rate (r) | Fixed at 0 — not meaningful in this domain |
| "Call" / "Put" | Which side of the threshold a MarketType defines as favorable — direction is chosen per market type, not hardcoded (see `core/market.py`'s `delta_sign`) |

Every `MarketType` supplies a `to_underlying` transform from its domain
value into a strictly-positive value Black-Scholes can price (see
`core/pricing.py`'s module docstring for why, and each market type's own
docstring for its specific transform — shuttle negates delay, election
does not, by design, to prove the core handles both).

## Known modeling limitations (documented in code, not hidden)

- **Shuttle arrival delay is bounded and right-skewed**, not a free random
  walk — see `market_types/shuttle_arrival/simulator.py`'s module
  docstring. The dashboard's Monte Carlo cross-check deliberately surfaces
  the resulting gap between the GBM-based price and a realistic bootstrap
  price rather than hiding it.
- **A vote-share campaign's live estimate gets noisier the further out you
  look**, without an upper cap — see `market_types/student_election/
  market_type.py`'s `live_spot` comment. This is a deliberate modeling
  choice (polls taken far from election day really are less informative),
  not an oversight.
- **No real authentication.** `/admin/markets` is a separate route prefix
  by convention, not backed by real access control, and `/portfolio` is a
  single global demo portfolio. Both are scope limitations, not oversights.
- **A binary (yes/no) contract kind was deliberately not built.** It would
  need a digital-option pricing formula (`e^{-rT}·N(d2)`, reusing
  `bs_engine`'s existing `d2`) and a new `contract_kind` the UI doesn't
  handle — flagged as a candidate for a future market type rather than
  forced into the current threshold-contract shape.

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

60 tests cover: the reused Black-Scholes engine, the generic core pricing
engine (via two toy transforms — one negated, one not), the generic
settlement state machine, SQLite persistence, both market types
(including a regression suite proving the shuttle migration didn't change
any numbers), and the FastAPI endpoints end-to-end across both markets.
