"""
Arrival-window confidence contract pricing.

Domain mapping (see project README for the full table):
    Spot (S)      -> current predicted arrival delay, transformed (see below)
    Strike (K)    -> a threshold delay window being evaluated, same transform
    Time (T)      -> hours remaining until the scheduled arrival
    Volatility    -> historical variance of delay for this route/hour,
                     calibrated by simulator.calibrate_sigma
    Rate (r)      -> always 0.0; not meaningful for this domain
    "Call"        -> confidence contract that the bus arrives ON TIME OR
                     EARLY relative to the threshold (delay < threshold)
    "Put"         -> confidence contract that the bus arrives LATE relative
                     to the threshold (delay > threshold)

Why the shift/negate transform exists
--------------------------------------
Black-Scholes requires a strictly positive, lognormal underlying (it takes
log(S/K)). Arrival delay can be zero or negative (an early bus), which
breaks that requirement outright. Rather than invent a new closed form, we
reuse the existing engine unchanged (bs_engine.py) and instead transform the
domain variable into something the engine can safely price:

    underlying = -delay + SHIFT_MINUTES
    strike     = -threshold + SHIFT_MINUTES

This is the same trick real rates desks used for "shifted Black" models
once interest rates went negative post-2015: shift the whole distribution
until it's positive, price with the standard formula, and the shift cancels
out of every payoff difference. Negating delay (rather than just shifting
it) is what makes "Call" correspond to "arrives before threshold": a
Black-Scholes call payoff max(underlying - strike, 0) becomes, after
substitution, max(threshold - delay, 0) — exactly a punctuality payoff.
SHIFT_MINUTES only needs to be large enough to keep both operands positive
across the simulator's delay range; it has no other effect on pricing
because the log-ratio, not the shift, drives the model's sensitivity.

Monte Carlo cross-checks operate directly in delay-space (no shift needed)
since they don't touch log(S/K) — see monte_carlo_gbm_price and
monte_carlo_bootstrap_price below.
"""
import math
from dataclasses import dataclass

import numpy as np

from . import bs_engine

SHIFT_MINUTES = 60.0
MIN_TIME_YEARS = 1e-6  # floor for T so d1/d2 never divide by zero


def _to_underlying(delay_minutes: float) -> float:
    return -delay_minutes + SHIFT_MINUTES


def minutes_to_years(minutes: float) -> float:
    """BS formulas are unit-agnostic as long as sigma is scaled to match T's
    units; we use hours-as-years internally so an annualized-style sigma
    (per sqrt(hour)) lines up with T measured in hours."""
    return max(minutes / 60.0, MIN_TIME_YEARS)


def payoff_call(delay_minutes: float, threshold_minutes: float) -> float:
    """Confidence payoff for 'arrives before threshold' (delay-space, no shift)."""
    return max(threshold_minutes - delay_minutes, 0.0)


def payoff_put(delay_minutes: float, threshold_minutes: float) -> float:
    """Confidence payoff for 'arrives after threshold' (delay-space, no shift)."""
    return max(delay_minutes - threshold_minutes, 0.0)


@dataclass
class ContractQuote:
    contract_type: str          # 'call' or 'put'
    price: float                # simulated confidence units, NOT real currency
    delta: float                # d(price)/d(predicted delay, minutes)
    gamma: float
    vega: float                 # d(price)/d(sigma)
    theta: float                # d(price)/d(time), value decay as T -> 0
    rho: float
    is_intrinsic: bool          # True once T has hit the near-expiry floor


def price_confidence_contract(predicted_delay_minutes: float, threshold_minutes: float,
                               minutes_remaining: float, sigma: float,
                               contract_type: str = 'call', r: float = 0.0) -> ContractQuote:
    """Price one arrival-window confidence contract via the shifted Black-Scholes engine."""
    if contract_type not in ('call', 'put'):
        raise ValueError("contract_type must be 'call' or 'put'")

    T = minutes_to_years(minutes_remaining)
    if minutes_remaining <= 1e-4:
        payoff_fn = payoff_call if contract_type == 'call' else payoff_put
        payoff = payoff_fn(predicted_delay_minutes, threshold_minutes)
        return ContractQuote(contract_type, payoff, 0.0, 0.0, 0.0, 0.0, 0.0, True)

    S = _to_underlying(predicted_delay_minutes)
    K = _to_underlying(threshold_minutes)
    price_fn = bs_engine.call_price if contract_type == 'call' else bs_engine.put_price
    price = price_fn(S, K, T, r, sigma)

    # dS/d(delay) = -1, so flip sign of the raw BS delta to express
    # sensitivity to predicted delay (rather than to the internal underlying).
    delay_delta = -bs_engine.delta(S, K, T, r, sigma, contract_type)
    return ContractQuote(
        contract_type=contract_type,
        price=price,
        delta=delay_delta,
        gamma=bs_engine.gamma(S, K, T, r, sigma),
        vega=bs_engine.vega(S, K, T, r, sigma),
        theta=bs_engine.theta(S, K, T, r, sigma, contract_type),
        rho=bs_engine.rho(S, K, T, r, sigma, contract_type),
        is_intrinsic=False,
    )


def implied_punctuality(quoted_price: float, predicted_delay_minutes: float,
                         threshold_minutes: float, minutes_remaining: float,
                         contract_type: str = 'call', r: float = 0.0) -> float:
    """
    Invert a quoted confidence price to back out "implied punctuality"
    (an implied sigma), comparable against the historically-calibrated
    sigma from simulator.calibrate_sigma. A large gap between implied and
    historical sigma means the quoted price is pricing in more/less
    uncertainty than the route's own history suggests.
    """
    T = minutes_to_years(minutes_remaining)
    S = _to_underlying(predicted_delay_minutes)
    K = _to_underlying(threshold_minutes)
    return bs_engine.implied_vol(quoted_price, S, K, T, r, option_type=contract_type)


def monte_carlo_gbm_price(predicted_delay_minutes: float, threshold_minutes: float,
                           minutes_remaining: float, sigma: float,
                           contract_type: str = 'call', r: float = 0.0,
                           simulations: int = 10000, rng=None) -> float:
    """
    Validates the analytical price under the engine's own GBM assumption.
    Should converge to price_confidence_contract's output as simulations
    grows — this checks our shift/negate algebra is correct, not whether
    GBM is realistic (it isn't; see monte_carlo_bootstrap_price for that).
    """
    T = minutes_to_years(minutes_remaining)
    S = _to_underlying(predicted_delay_minutes)
    K = _to_underlying(threshold_minutes)
    return bs_engine.monte_carlo(S, K, T, r, sigma, contract_type, simulations, rng)


def monte_carlo_bootstrap_price(threshold_minutes: float, historical_delays: np.ndarray,
                                 predicted_delay_minutes: float, contract_type: str = 'call',
                                 n_resamples: int = 10000, rng=None) -> float:
    """
    The realistic cross-check: bootstrap-resample actual historical delays
    (recentered on today's live prediction) instead of drawing from a
    lognormal GBM. Bus delay is bounded and right-skewed (see simulator.py's
    module docstring), so this and monte_carlo_gbm_price / the analytical
    price will *not* fully agree — the gap is the model risk of using
    Black-Scholes' lognormal assumption on a process that isn't lognormal.
    """
    rng = rng or np.random.default_rng()
    recenter = predicted_delay_minutes - float(np.mean(historical_delays))
    sample = rng.choice(historical_delays, size=n_resamples, replace=True) + recenter
    payoff_fn = payoff_call if contract_type == 'call' else payoff_put
    payoffs = np.array([payoff_fn(d, threshold_minutes) for d in sample])
    return float(np.mean(payoffs))
