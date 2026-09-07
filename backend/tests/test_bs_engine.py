"""Sanity checks against the original engine's known-good numbers (see
blacksholes/README.md: S=100,K=100,T=1,r=0.05,sigma=0.2 -> Call 10.45, Put 5.57)."""
import numpy as np
from app import bs_engine


def test_matches_original_engine_sanity_check():
    S, K, T, r, sigma = 100, 100, 1, 0.05, 0.2
    assert round(bs_engine.call_price(S, K, T, r, sigma), 2) == 10.45
    assert round(bs_engine.put_price(S, K, T, r, sigma), 2) == 5.57
    assert round(bs_engine.delta(S, K, T, r, sigma), 3) == 0.637
    assert round(bs_engine.gamma(S, K, T, r, sigma), 4) == 0.0188
    assert round(bs_engine.vega(S, K, T, r, sigma), 2) == 37.52
    assert round(bs_engine.theta(S, K, T, r, sigma), 2) == -6.41
    assert round(bs_engine.rho(S, K, T, r, sigma), 2) == 53.23


def test_monte_carlo_converges_to_analytical():
    S, K, T, r, sigma = 100, 100, 1, 0.05, 0.2
    analytical = bs_engine.call_price(S, K, T, r, sigma)
    mc = bs_engine.monte_carlo(S, K, T, r, sigma, 'call', simulations=200_000,
                                rng=np.random.default_rng(0))
    assert abs(mc - analytical) < 0.15


def test_implied_vol_recovers_known_sigma():
    S, K, T, r, sigma = 100, 100, 1, 0.05, 0.2
    price = bs_engine.call_price(S, K, T, r, sigma)
    recovered = bs_engine.implied_vol(price, S, K, T, r, 'call')
    assert abs(recovered - sigma) < 1e-4


def test_implied_vol_does_not_hang_on_bad_input():
    # a market price far outside any achievable BS price used to loop forever
    # in the original engine; now it should bail out via max_iter.
    result = bs_engine.implied_vol(1e6, 100, 100, 1, 0.05, 'call')
    assert np.isfinite(result)
