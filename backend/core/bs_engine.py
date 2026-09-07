"""
Generic Black-Scholes / Greeks / Monte Carlo engine.

Reused near-verbatim from the existing options pricing engine
(blacksholes/blackscholes.py). This module knows nothing about shuttles —
it is pure derivatives-pricing math on abstract (S, K, T, r, sigma).
Domain-specific meaning (arrival times, thresholds, punctuality) lives in
pricing.py, which wraps these functions.

Adaptations vs. the original:
- implied_vol now has a max-iteration cap and a vega floor so Newton-Raphson
  can't divide by ~0 or loop forever on bad inputs (the original could).
- monte_carlo/simulate_paths are vectorized with numpy instead of Python
  while-loops, since the dashboard needs to reprice on every slider move.
"""
import math
import numpy as np


def N(x):
    return 0.5 * (1 + math.erf(x / math.sqrt(2)))


def N_prime(x):
    return math.exp(-x ** 2 / 2) / math.sqrt(2 * math.pi)


def d1(S, K, T, r, sigma):
    return (math.log(S / K) + (r + 0.5 * sigma ** 2) * T) / (sigma * math.sqrt(T))


def d2(S, K, T, r, sigma):
    return d1(S, K, T, r, sigma) - sigma * math.sqrt(T)


def call_price(S, K, T, r, sigma):
    return S * N(d1(S, K, T, r, sigma)) - K * math.exp(-r * T) * N(d2(S, K, T, r, sigma))


def put_price(S, K, T, r, sigma):
    return -S * N(-d1(S, K, T, r, sigma)) + K * math.exp(-r * T) * N(-d2(S, K, T, r, sigma))


def delta(S, K, T, r, sigma, option_type='call'):
    if option_type == 'call':
        return N(d1(S, K, T, r, sigma))
    return N(d1(S, K, T, r, sigma)) - 1


def gamma(S, K, T, r, sigma):
    return N_prime(d1(S, K, T, r, sigma)) / (S * sigma * math.sqrt(T))


def vega(S, K, T, r, sigma):
    return S * N_prime(d1(S, K, T, r, sigma)) * math.sqrt(T)


def theta(S, K, T, r, sigma, option_type='call'):
    common = (-S * N_prime(d1(S, K, T, r, sigma)) * sigma) / (2 * math.sqrt(T))
    if option_type == 'call':
        return common - r * K * math.exp(-r * T) * N(d2(S, K, T, r, sigma))
    return common + r * K * math.exp(-r * T) * N(-d2(S, K, T, r, sigma))


def rho(S, K, T, r, sigma, option_type='call'):
    if option_type == 'call':
        return K * T * math.exp(-r * T) * N(d2(S, K, T, r, sigma))
    return -K * T * math.exp(-r * T) * N(-d2(S, K, T, r, sigma))


def simulate_terminal_prices(S, r, sigma, T, n_paths, rng=None):
    """Vectorized GBM terminal-value draws (replaces the per-step Python loop)."""
    rng = rng or np.random.default_rng()
    z = rng.standard_normal(n_paths)
    return S * np.exp((r - 0.5 * sigma ** 2) * T + sigma * math.sqrt(T) * z)


def monte_carlo(S, K, T, r, sigma, option_type='call', simulations=10000, rng=None):
    finals = simulate_terminal_prices(S, r, sigma, T, simulations, rng)
    if option_type == 'call':
        payoffs = np.maximum(finals - K, 0)
    else:
        payoffs = np.maximum(K - finals, 0)
    return float(np.mean(payoffs) * math.exp(-r * T))


def implied_vol(market_price, S, K, T, r, option_type='call', max_iter=100, tol=1e-6):
    sigma = 0.2
    for _ in range(max_iter):
        price_fn = call_price if option_type == 'call' else put_price
        price_bs = price_fn(S, K, T, r, sigma)
        diff = price_bs - market_price
        if abs(diff) < tol:
            return sigma
        v = vega(S, K, T, r, sigma)
        if v < 1e-8:
            break
        sigma = sigma - diff / v
        sigma = max(sigma, 1e-4)
    return sigma
