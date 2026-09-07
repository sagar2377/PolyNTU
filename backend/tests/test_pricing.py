import numpy as np
from app import pricing, simulator


def test_call_is_worth_more_when_comfortably_before_threshold():
    near_miss = pricing.price_confidence_contract(
        predicted_delay_minutes=4.0, threshold_minutes=5.0,
        minutes_remaining=20, sigma=0.2, contract_type='call')
    comfortable = pricing.price_confidence_contract(
        predicted_delay_minutes=-2.0, threshold_minutes=5.0,
        minutes_remaining=20, sigma=0.2, contract_type='call')
    assert comfortable.price > near_miss.price


def test_put_is_worth_more_when_predicted_delay_exceeds_threshold():
    on_time = pricing.price_confidence_contract(
        predicted_delay_minutes=0.0, threshold_minutes=5.0,
        minutes_remaining=20, sigma=0.2, contract_type='put')
    very_late = pricing.price_confidence_contract(
        predicted_delay_minutes=15.0, threshold_minutes=5.0,
        minutes_remaining=20, sigma=0.2, contract_type='put')
    assert very_late.price > on_time.price


def test_intrinsic_value_at_expiry_matches_payoff_functions():
    q_call = pricing.price_confidence_contract(
        predicted_delay_minutes=3.0, threshold_minutes=5.0,
        minutes_remaining=0.0, sigma=0.2, contract_type='call')
    assert q_call.is_intrinsic
    assert q_call.price == pricing.payoff_call(3.0, 5.0) == 2.0

    q_put = pricing.price_confidence_contract(
        predicted_delay_minutes=8.0, threshold_minutes=5.0,
        minutes_remaining=0.0, sigma=0.2, contract_type='put')
    assert q_put.is_intrinsic
    assert q_put.price == pricing.payoff_put(8.0, 5.0) == 3.0


def test_gbm_monte_carlo_matches_analytical_price():
    kwargs = dict(predicted_delay_minutes=2.0, threshold_minutes=5.0,
                   minutes_remaining=15, sigma=0.2, contract_type='call')
    analytical = pricing.price_confidence_contract(**kwargs).price
    mc = pricing.monte_carlo_gbm_price(**kwargs, simulations=200_000,
                                        rng=np.random.default_rng(1))
    assert abs(mc - analytical) < 0.2


def test_bootstrap_monte_carlo_diverges_from_gbm_when_distribution_is_skewed():
    """
    This is the modeling-limitation check: bus delay is bounded/right-skewed,
    not lognormal, so the empirical bootstrap price should meaningfully
    differ from the GBM-based price for a threshold near the distribution's
    boundary. If this test ever shows them converging, the skew assumption
    in simulator.py's HOURLY_PROFILES has likely been changed to something
    closer to symmetric/lognormal.
    """
    rng = np.random.default_rng(2)
    historical = simulator.generate_historical_delays('R1', hour_of_day=8, n_days=500, rng=rng)
    predicted_delay = 2.0
    threshold = 0.0  # near the lower boundary where skew bites hardest
    sigma = simulator.calibrate_sigma(historical)

    gbm_price = pricing.monte_carlo_gbm_price(
        predicted_delay, threshold, minutes_remaining=15, sigma=sigma,
        contract_type='put', simulations=200_000, rng=rng)
    bootstrap_price = pricing.monte_carlo_bootstrap_price(
        threshold, historical, predicted_delay, contract_type='put',
        n_resamples=200_000, rng=rng)

    assert abs(gbm_price - bootstrap_price) > 0.1


def test_implied_punctuality_round_trips_a_known_sigma():
    sigma = 0.2
    kwargs = dict(predicted_delay_minutes=2.0, threshold_minutes=5.0,
                   minutes_remaining=15, contract_type='call')
    quote = pricing.price_confidence_contract(**kwargs, sigma=sigma)
    recovered = pricing.implied_punctuality(quote.price, **kwargs)
    assert abs(recovered - sigma) < 1e-3


def test_delta_is_positive_for_call_and_negative_for_put():
    call = pricing.price_confidence_contract(2.0, 5.0, 15, 0.2, 'call')
    put = pricing.price_confidence_contract(2.0, 5.0, 15, 0.2, 'put')
    # a rising predicted delay should hurt a "before threshold" call...
    assert call.delta < 0
    # ...and help an "after threshold" put.
    assert put.delta > 0
