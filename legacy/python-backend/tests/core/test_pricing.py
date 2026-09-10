"""
Generic tests for core.pricing using two toy transforms, to prove the
engine itself is market-agnostic:
  - `negated`: to_underlying(v) = -v + 50, delta_sign=-1 (shuttle-style:
    "call" means the domain value ends up BELOW the threshold).
  - `identity`: to_underlying(v) = v + 0.01, delta_sign=+1 (election-style:
    "call" means the domain value ends up ABOVE the threshold).
Market-type-specific regression tests (shuttle's exact numbers, election's
exact numbers) live under tests/market_types/*/test_market_type.py.
"""
import numpy as np
from core import pricing


def negated_to_underlying(v):
    return -v + 50.0


def negated_payoff_call(realized, threshold):
    return max(threshold - realized, 0.0)


def negated_payoff_put(realized, threshold):
    return max(realized - threshold, 0.0)


def identity_to_underlying(v):
    return max(v, 0.0) + 0.01


def identity_payoff_call(realized, threshold):
    return max(realized - threshold, 0.0)


def identity_payoff_put(realized, threshold):
    return max(threshold - realized, 0.0)


def test_negated_transform_call_worth_more_when_comfortably_below_threshold():
    near_miss = pricing.price_threshold_contract(
        4.0, 5.0, 20, 0.2, 'call', negated_to_underlying, negated_payoff_call, delta_sign=-1.0)
    comfortable = pricing.price_threshold_contract(
        -2.0, 5.0, 20, 0.2, 'call', negated_to_underlying, negated_payoff_call, delta_sign=-1.0)
    assert comfortable.price > near_miss.price
    assert comfortable.delta < 0  # rising value hurts a "must stay below" call


def test_identity_transform_call_worth_more_when_comfortably_above_threshold():
    near_miss = pricing.price_threshold_contract(
        4.0, 5.0, 20, 0.2, 'call', identity_to_underlying, identity_payoff_call, delta_sign=1.0)
    comfortable = pricing.price_threshold_contract(
        12.0, 5.0, 20, 0.2, 'call', identity_to_underlying, identity_payoff_call, delta_sign=1.0)
    assert comfortable.price > near_miss.price
    assert comfortable.delta > 0  # rising value helps a "must exceed" call


def test_intrinsic_value_at_expiry_uses_payoff_fn_directly():
    q = pricing.price_threshold_contract(
        3.0, 5.0, 0.0, 0.2, 'call', negated_to_underlying, negated_payoff_call, delta_sign=-1.0)
    assert q.is_intrinsic
    assert q.price == negated_payoff_call(3.0, 5.0) == 2.0


def test_gbm_monte_carlo_matches_analytical_price():
    kwargs = dict(spot=2.0, threshold=5.0, minutes_remaining=15, sigma=0.2, contract_type='call')
    analytical = pricing.price_threshold_contract(
        **kwargs, to_underlying=negated_to_underlying, payoff_fn=negated_payoff_call, delta_sign=-1.0).price
    mc = pricing.monte_carlo_gbm_price(**kwargs, to_underlying=negated_to_underlying,
                                        simulations=200_000, rng=np.random.default_rng(1))
    assert abs(mc - analytical) < 0.2


def test_bootstrap_monte_carlo_operates_in_domain_space():
    rng = np.random.default_rng(2)
    historical = rng.gamma(2.0, 3.0, size=5000) - 2.0  # right-skewed, not lognormal
    spot = 2.0
    threshold = 0.0
    price = pricing.monte_carlo_bootstrap_price(threshold, historical, spot, negated_payoff_call,
                                                 n_resamples=50_000, rng=rng)
    assert price >= 0


def test_implied_sigma_round_trips_a_known_sigma():
    sigma = 0.2
    kwargs = dict(spot=2.0, threshold=5.0, minutes_remaining=15, contract_type='call')
    quote = pricing.price_threshold_contract(
        **kwargs, sigma=sigma, to_underlying=negated_to_underlying,
        payoff_fn=negated_payoff_call, delta_sign=-1.0)
    recovered = pricing.implied_sigma(quote.price, **kwargs, to_underlying=negated_to_underlying)
    assert abs(recovered - sigma) < 1e-3
