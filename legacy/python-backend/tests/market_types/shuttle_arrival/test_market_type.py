"""
Regression tests: these are the exact scenarios from ShuttlePredict's
original tests/test_pricing.py, re-run through the new
ShuttleArrivalMarketType + core.pricing path. Confirms the migration into
the plugin interface didn't change any numbers — same to_underlying
(-delay + 60), same payoff formulas, same delta_sign (-1.0).
"""
import numpy as np
from core import pricing
from market_types.shuttle_arrival.market_type import ShuttleArrivalMarketType
from market_types.shuttle_arrival import simulator

MT = ShuttleArrivalMarketType()


def quote(spot, threshold, minutes_remaining, sigma, contract_type):
    payoff_fn = MT.payoff_call if contract_type == 'call' else MT.payoff_put
    return pricing.price_threshold_contract(
        spot, threshold, minutes_remaining, sigma, contract_type,
        to_underlying=MT.to_underlying, payoff_fn=payoff_fn, delta_sign=MT.delta_sign)


def test_call_is_worth_more_when_comfortably_before_threshold():
    near_miss = quote(4.0, 5.0, 20, 0.2, 'call')
    comfortable = quote(-2.0, 5.0, 20, 0.2, 'call')
    assert comfortable.price > near_miss.price


def test_put_is_worth_more_when_predicted_delay_exceeds_threshold():
    on_time = quote(0.0, 5.0, 20, 0.2, 'put')
    very_late = quote(15.0, 5.0, 20, 0.2, 'put')
    assert very_late.price > on_time.price


def test_intrinsic_value_at_expiry_matches_payoff_functions():
    q_call = quote(3.0, 5.0, 0.0, 0.2, 'call')
    assert q_call.is_intrinsic
    assert q_call.price == MT.payoff_call(3.0, 5.0) == 2.0

    q_put = quote(8.0, 5.0, 0.0, 0.2, 'put')
    assert q_put.is_intrinsic
    assert q_put.price == MT.payoff_put(8.0, 5.0) == 3.0


def test_gbm_monte_carlo_matches_analytical_price():
    spot, threshold, minutes_remaining, sigma = 2.0, 5.0, 15, 0.2
    analytical = quote(spot, threshold, minutes_remaining, sigma, 'call').price
    mc = pricing.monte_carlo_gbm_price(spot, threshold, minutes_remaining, sigma, 'call',
                                        to_underlying=MT.to_underlying,
                                        simulations=200_000, rng=np.random.default_rng(1))
    assert abs(mc - analytical) < 0.2


def test_bootstrap_monte_carlo_diverges_from_gbm_when_distribution_is_skewed():
    """
    The modeling-limitation check: bus delay is bounded/right-skewed, not
    lognormal, so the empirical bootstrap price should meaningfully differ
    from the GBM-based price for a threshold near the distribution's
    boundary.
    """
    rng = np.random.default_rng(2)
    historical = simulator.generate_historical_delays('R1', hour_of_day=8, n_days=500, rng=rng)
    from core.calibration import calibrate_sigma
    from market_types.shuttle_arrival.market_type import SHIFT_MINUTES
    sigma = calibrate_sigma(historical, reference_horizon_minutes=10.0, reference_level=SHIFT_MINUTES)
    spot, threshold = 2.0, 0.0

    gbm_price = pricing.monte_carlo_gbm_price(spot, threshold, 15, sigma, 'put',
                                               to_underlying=MT.to_underlying,
                                               simulations=200_000, rng=rng)
    bootstrap_price = pricing.monte_carlo_bootstrap_price(threshold, historical, spot, MT.payoff_put,
                                                           n_resamples=200_000, rng=rng)
    assert abs(gbm_price - bootstrap_price) > 0.1


def test_implied_sigma_round_trips_a_known_sigma():
    sigma = 0.2
    spot, threshold, minutes_remaining = 2.0, 5.0, 15
    q = quote(spot, threshold, minutes_remaining, sigma, 'call')
    recovered = pricing.implied_sigma(q.price, spot, threshold, minutes_remaining, 'call',
                                       to_underlying=MT.to_underlying)
    assert abs(recovered - sigma) < 1e-3


def test_delta_sign_matches_punctuality_intuition():
    call = quote(2.0, 5.0, 15, 0.2, 'call')
    put = quote(2.0, 5.0, 15, 0.2, 'put')
    assert call.delta < 0   # rising delay hurts a "arrives before" call
    assert put.delta > 0    # rising delay helps an "arrives after" put


def test_contract_labels_and_calibration_bucket():
    assert MT.contract_label('call') == "arrives before threshold"
    assert MT.contract_label('put') == "arrives after threshold"
    assert MT.calibration_bucket(8 * 60 + 30) == "8"
