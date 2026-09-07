# Adding a new Market Type

A Market Type is a plugin under `backend/market_types/<key>/` with two
files: `simulator.py` (your domain's data-generation logic) and
`market_type.py` (a `MarketType` subclass wired into `core/market.py`'s
interface). Nothing in `core/` ever imports a concrete market type — the
core pricing engine, settlement state machine, and DB layer are all
generic over the `MarketType` interface defined in `core/market.py`.

Use `market_types/shuttle_arrival/` (negated transform, many short-lived
recurring instances) and `market_types/student_election/` (identity
transform, one long-lived instance) as the two worked examples — between
them they exercise both directions of every design decision below.

## Checklist

1. **Pick your domain value and its units.** What is the "spot price" a
   user sees before resolution? (Shuttle: predicted delay in minutes.
   Election: estimated vote-share percentage.) Set `unit_label` on the
   `Market` you create to whatever this is, in plain words.

2. **Write `to_underlying(value) -> float`.** Black-Scholes needs a
   strictly positive underlying (it computes `log(S/K)`). If your domain
   value can be zero or negative, you need an affine shift (and, if the
   "good" outcome is a *low* value, a negation too — see
   `market_types/shuttle_arrival/market_type.py`'s module docstring for
   the full derivation of why negation makes "Call" mean "ends up below
   threshold"). If your value is already non-negative, a small positivity
   floor is enough (see `student_election`'s `EPSILON`).

3. **Set `delta_sign`.** `+1.0` if `to_underlying` doesn't negate, `-1.0`
   if it does. This is the *only* Greek that needs a sign correction —
   gamma is invariant to an affine transform's sign, and vega/theta/rho
   don't depend on the value-transform at all (see `core/pricing.py`'s
   module docstring for why).

4. **Write `payoff_call`/`payoff_put` in domain units.** These are used
   both for the near-expiry pricing shortcut and for real settlement —
   keep them simple, direct `max(...)` expressions in your domain's native
   units. They do NOT need to go through `to_underlying` (the shift cancels
   out of any payoff difference).

5. **Write `generate_instances(market, rng) -> list[MarketInstance]`.**
   Simulate your domain's schedule of resolvable events. Each
   `MarketInstance` needs a `resolution_minute` and a `truth_seed` dict
   holding whatever ground truth `resolve()` will need later. Instances
   are NOT persisted — they're regenerated deterministically from a seeded
   RNG each time the platform starts, same as ShuttlePredict's original
   schedule generator.

6. **Write `live_spot(market, instance, now_minute, rng, observations) ->
   (spot, sigma)`.** If your domain value should get noisier the further
   `now_minute` is from resolution, reuse
   `core.live_estimate.brownian_bridge_estimate` rather than reimplementing
   it. Calibrate `sigma` from `observations` via `core.calibration.
   calibrate_sigma` — pick a `reference_level` that matches whatever level
   your `to_underlying` transform puts values at (get this wrong and every
   price and Greek is silently miscalibrated; see that function's
   docstring).

7. **Write `resolve(market, instance) -> float`.** Usually just reads back
   a value from `instance.truth_seed`.

8. **Override `contract_label`, `greek_hints`, `calibration_bucket`, and
   `validate_params`** as needed — all have generic defaults in
   `MarketType`, but a specific market type should give the UI
   domain-flavored wording (see both plugins' overrides for the pattern).

9. **Add a `seed_demo_history` override if you want fake historical data**
   for calibration in the demo (both existing plugins do this). A real
   deployment would rely on accumulating real logs instead — this hook is
   optional and no-op by default.

10. **Register it.** Add `from .<your_type>.market_type import
    YourMarketType` and `market.register(YourMarketType())` to
    `backend/market_types/__init__.py`.

11. **Add a default `Market` instance** to `DEFAULT_MARKETS` in
    `backend/app/main.py` if you want it seeded automatically, or create
    one via `POST /admin/markets` (which runs your `validate_params` and
    rejects an empty/placeholder `resolution_criterion` before it goes
    live).

12. **Write tests** under `tests/market_types/<key>/`: a `test_simulator.py`
    for your data generation, and a `test_market_type.py` that exercises
    `to_underlying`/`payoff_call`/`payoff_put`/`generate_instances`/
    `resolve`/`live_spot` — see either existing plugin's tests as a
    template, including the "does this converge near resolution"
    convergence check.

## A responsible-design constraint worth keeping

If your market type could ever touch a real person's reputation, a real
in-progress process, or anything where being "priced" without consent
would cause harm (elections, disputes, individual performance, etc.),
consider whether `validate_params` should hard-require an explicit opt-in
flag the way `student_election` requires `is_fictional: true`. It costs
one `if` statement and prevents the plugin from being pointed at something
it was never designed to handle safely.
