"""
Live estimate of a not-yet-known true value — market-agnostic.

Originally lived in the shuttle simulator as "predicted_delay_minutes," but
the pattern has no shuttle-specific content: any market type showing a
rider/voter/etc. a live estimate of an outcome that will only be known for
certain at resolution time needs a signal that starts noisy and converges
to the true value as time-to-resolution shrinks to zero. Modeled as a
Brownian-bridge-style noise term scaled by remaining time (Brownian
scaling), consistent with how sigma is calibrated in calibration.py.
"""
import numpy as np


def brownian_bridge_estimate(true_value: float, minutes_remaining: float,
                              minutes_to_horizon: float, sigma_abs: float,
                              rng=None) -> float:
    """
    A live estimate of `true_value` at some point before resolution: the
    true value plus noise that shrinks to zero as minutes_remaining -> 0.

    sigma_abs is the ABSOLUTE (domain-unit) noise per sqrt(hour) of time
    remaining — i.e. a fractional sigma from calibrate_sigma multiplied
    back up by the same reference_level used to calibrate it.
    """
    rng = rng or np.random.default_rng()
    if minutes_remaining <= 0:
        return true_value
    hours_remaining = min(minutes_remaining, minutes_to_horizon) / 60.0
    noise = rng.normal(0, sigma_abs * np.sqrt(hours_remaining))
    return true_value + noise
