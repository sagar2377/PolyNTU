"""
Shuttle Arrival market type.

Domain mapping:
    Spot (S)      -> current live-estimated arrival delay (minutes), transformed
    Strike (K)    -> a threshold delay window (minutes), same transform
    Volatility    -> historical variance of delay for this route/hour
    "Call"        -> confidence contract that the bus arrives ON TIME OR
                     EARLY relative to the threshold (delay < threshold)
    "Put"         -> confidence contract that the bus arrives LATE relative
                     to the threshold (delay > threshold)

Why the shift/negate transform: Black-Scholes requires a strictly
positive, lognormal underlying (it takes log(S/K)). Arrival delay can be
zero or negative (an early bus), which breaks that requirement outright.
Rather than invent a new closed form, we transform the domain variable
into something core/pricing.py can safely price:

    underlying = -delay + SHIFT_MINUTES

This is the same trick real rates desks used for "shifted Black" models
once interest rates went negative post-2015. Negating delay (rather than
just shifting it) is what makes "Call" correspond to "arrives before
threshold": a Black-Scholes call payoff max(underlying - strike, 0)
becomes, after substitution, max(threshold - delay, 0) — a punctuality
payoff. SHIFT_MINUTES only needs to be large enough to keep both operands
positive across the simulator's delay range.
"""
import numpy as np

from core.calibration import calibrate_sigma
from core.live_estimate import brownian_bridge_estimate
from core.market import Market, MarketInstance, MarketType

from . import simulator

SHIFT_MINUTES = 60.0
CALIBRATION_HOURS = list(range(7, 23))  # matches the demo service window, 7am-10pm


class ShuttleArrivalMarketType(MarketType):
    key = "shuttle_arrival"
    contract_kind = "threshold"
    delta_sign = -1.0   # to_underlying negates delay

    def to_underlying(self, delay_minutes: float) -> float:
        return -delay_minutes + SHIFT_MINUTES

    def payoff_call(self, realized_delay: float, threshold: float) -> float:
        return max(threshold - realized_delay, 0.0)

    def payoff_put(self, realized_delay: float, threshold: float) -> float:
        return max(realized_delay - threshold, 0.0)

    def contract_label(self, contract_type: str) -> str:
        return "arrives before threshold" if contract_type == "call" else "arrives after threshold"

    def greek_hints(self) -> dict[str, str]:
        return {
            "delta": "sensitivity to the current predicted arrival time moving",
            "gamma": "sensitivity of delta itself to the predicted arrival time",
            "vega": "sensitivity to route volatility (punctuality uncertainty)",
            "theta": "value decay as the scheduled time approaches",
            "rho": "not meaningful here (rate is fixed at 0)",
        }

    def validate_params(self, params: dict) -> list[str]:
        errors = []
        if "route_id" not in params:
            errors.append("shuttle_arrival markets require params.route_id")
        return errors

    def calibration_bucket(self, resolution_minute: int) -> str:
        return str((resolution_minute // 60) % 24)

    def generate_instances(self, market: Market, rng) -> list[MarketInstance]:
        route_id = market.params["route_id"]
        trips = simulator.generate_daily_schedule(
            route_id,
            service_start_minute=market.params.get("service_start_minute", 7 * 60),
            service_end_minute=market.params.get("service_end_minute", 22 * 60),
            rng=rng,
        )
        return [
            MarketInstance(
                id=f"{market.id}:{t.scheduled_minute_of_day}",
                market_id=market.id,
                resolution_minute=t.scheduled_minute_of_day,
                truth_seed={"true_delay_minutes": t.true_delay_minutes},
            )
            for t in trips
        ]

    def live_spot(self, market: Market, instance: MarketInstance, now_minute: int,
                   rng, observations: list[float]) -> tuple[float, float]:
        minutes_remaining = max(instance.resolution_minute - now_minute, 0)
        if len(observations) >= 10:
            sigma = calibrate_sigma(np.asarray(observations), reference_horizon_minutes=10.0,
                                     reference_level=SHIFT_MINUTES)
        else:
            sigma = 0.2  # fallback for a route/hour with no logged history yet
        predicted = brownian_bridge_estimate(
            instance.truth_seed["true_delay_minutes"], minutes_remaining,
            minutes_to_horizon=60, sigma_abs=sigma * SHIFT_MINUTES, rng=rng)
        return predicted, sigma

    def resolve(self, market: Market, instance: MarketInstance) -> float:
        return instance.truth_seed["true_delay_minutes"]

    def seed_demo_history(self, market: Market, rng, log_observation) -> None:
        route_id = market.params["route_id"]
        for hour in CALIBRATION_HOURS:
            delays = simulator.generate_historical_delays(route_id, hour, n_days=60, rng=rng)
            for d in delays:
                log_observation(str(hour), float(d))
