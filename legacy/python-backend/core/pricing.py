"""
Generic threshold-contract pricing on an abstract, market-supplied
underlying.

This module does NOT know what the underlying represents (arrival delay,
queue length, vote share, ...) or which direction is favorable — every
MarketType (market_types/*/market_type.py) supplies:
  1. `to_underlying`: a transform from its domain value into a strictly
     positive underlying safe for Black-Scholes' log(S/K) — e.g. shuttle's
     shift+negate trick (see market_types/shuttle_arrival/market_type.py).
  2. `payoff_fn` (payoff_call/payoff_put): the domain-space intrinsic
     payoff, used directly at the near-expiry floor and for settlement.
  3. `delta_sign`: dUnderlying/dValue (+1 or -1) so the returned delta is
     expressed w.r.t. the market's native value, not the internal
     transform. Because every `to_underlying` in this codebase is affine
     (a shift, optionally negated), only delta needs this correction —
     gamma is invariant to the transform's sign (it's a second derivative
     of an affine function), and vega/theta/rho don't depend on the
     value-transform at all, since they're derivatives w.r.t. sigma/T.

Reuses bs_engine.py's Black-Scholes/Greeks/Monte Carlo math completely
unchanged — this file is purely about how a MarketType's domain value gets
in and out of that math.
"""
from dataclasses import dataclass

import numpy as np

from . import bs_engine

MIN_TIME_YEARS = 1e-6  # floor for T so d1/d2 never divide by zero


def minutes_to_years(minutes: float) -> float:
    """BS formulas are unit-agnostic as long as sigma is scaled to match T's
    units; we use hours-as-years internally so a Brownian-scaled sigma
    (per sqrt(hour)) lines up with T measured in hours."""
    return max(minutes / 60.0, MIN_TIME_YEARS)


@dataclass
class ContractQuote:
    contract_type: str          # 'call' or 'put'
    price: float                # simulated confidence units, NOT real currency
    delta: float                # d(price)/d(domain value)
    gamma: float
    vega: float                 # d(price)/d(sigma)
    theta: float                # d(price)/d(time), value decay as T -> 0
    rho: float
    is_intrinsic: bool          # True once T has hit the near-expiry floor


def price_threshold_contract(spot: float, threshold: float, minutes_remaining: float,
                              sigma: float, contract_type: str, to_underlying, payoff_fn,
                              delta_sign: float = 1.0, r: float = 0.0) -> ContractQuote:
    """
    Prices one threshold contract. `spot`/`threshold` are in the MARKET'S
    OWN domain units; `to_underlying`/`payoff_fn` are supplied by the
    MarketType (see module docstring). Near-expiry short-circuits to
    payoff_fn(spot, threshold) directly in domain units — the shift used by
    to_underlying cancels out of the analytical formula but isn't needed at
    all for a literal payoff at resolution.
    """
    if contract_type not in ('call', 'put'):
        raise ValueError("contract_type must be 'call' or 'put'")

    minutes_remaining = max(minutes_remaining, 0.0)
    if minutes_remaining <= 1e-4:
        payoff = payoff_fn(spot, threshold)
        return ContractQuote(contract_type, payoff, 0.0, 0.0, 0.0, 0.0, 0.0, True)

    T = minutes_to_years(minutes_remaining)
    U = to_underlying(spot)
    K = to_underlying(threshold)
    price_fn = bs_engine.call_price if contract_type == 'call' else bs_engine.put_price
    price = price_fn(U, K, T, r, sigma)
    bs_delta = bs_engine.delta(U, K, T, r, sigma, contract_type)

    return ContractQuote(
        contract_type=contract_type,
        price=price,
        delta=delta_sign * bs_delta,
        gamma=bs_engine.gamma(U, K, T, r, sigma),
        vega=bs_engine.vega(U, K, T, r, sigma),
        theta=bs_engine.theta(U, K, T, r, sigma, contract_type),
        rho=bs_engine.rho(U, K, T, r, sigma, contract_type),
        is_intrinsic=False,
    )


def implied_sigma(quoted_price: float, spot: float, threshold: float, minutes_remaining: float,
                   contract_type: str, to_underlying, r: float = 0.0) -> float:
    """
    Invert a quoted price to back out "implied volatility" for this market
    (comparable against its historically-calibrated sigma). A large gap
    means the quoted price is pricing in more/less uncertainty than the
    market's own history suggests.
    """
    T = minutes_to_years(minutes_remaining)
    U = to_underlying(spot)
    K = to_underlying(threshold)
    return bs_engine.implied_vol(quoted_price, U, K, T, r, option_type=contract_type)


def monte_carlo_gbm_price(spot: float, threshold: float, minutes_remaining: float, sigma: float,
                           contract_type: str, to_underlying, r: float = 0.0,
                           simulations: int = 10000, rng=None) -> float:
    """
    Validates the analytical price under the engine's own GBM assumption.
    Should converge to price_threshold_contract's output as simulations
    grows — this checks the to_underlying/payoff algebra is correct, not
    whether GBM is realistic for this market (see monte_carlo_bootstrap_price).
    """
    T = minutes_to_years(minutes_remaining)
    U = to_underlying(spot)
    K = to_underlying(threshold)
    return bs_engine.monte_carlo(U, K, T, r, sigma, contract_type, simulations, rng)


def monte_carlo_bootstrap_price(threshold: float, historical_values: np.ndarray, spot: float,
                                 payoff_fn, n_resamples: int = 10000, rng=None) -> float:
    """
    The realistic cross-check: bootstrap-resample actual historical domain
    values (recentered on today's live estimate) instead of drawing from a
    lognormal GBM. For markets whose domain value is bounded/skewed (e.g.
    shuttle delay), this and monte_carlo_gbm_price / the analytical price
    will *not* fully agree — the gap is the model risk of applying
    Black-Scholes' lognormal assumption to a process that isn't lognormal.
    Operates entirely in domain space, so it needs no to_underlying at all.
    """
    rng = rng or np.random.default_rng()
    recenter = spot - float(np.mean(historical_values))
    sample = rng.choice(historical_values, size=n_resamples, replace=True) + recenter
    payoffs = np.array([payoff_fn(v, threshold) for v in sample])
    return float(np.mean(payoffs))
