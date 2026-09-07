"""
Volatility calibration — market-agnostic.

Originally lived in the shuttle simulator, but turned out to have no
shuttle-specific content once the reference level is a parameter: this is
just "absolute stdev of some domain quantity -> the fractional volatility
Black-Scholes expects," which every market type needs regardless of
whether the quantity is minutes of delay, people in a queue, or percentage
points of vote share.
"""
import numpy as np


def calibrate_sigma(historical_values: np.ndarray, reference_horizon_minutes: float,
                     reference_level: float) -> float:
    """
    Turn a sample of historical domain values into a Black-Scholes-style
    fractional volatility.

    Black-Scholes' sigma is a *fractional* volatility (it multiplies the
    underlying's level inside an exp()), not an absolute one — the same way
    equity vol of "20%" means 20% of the stock price, not a flat $20.
    calibrate_sigma does two conversions from the raw standard deviation of
    historical domain values:
      1. divide by reference_level to turn an absolute spread into a
         fraction of the underlying's level. This MUST match the level the
         calling MarketType's `to_underlying` transform puts values at
         (e.g. shuttle's SHIFT_MINUTES) — a mismatched reference_level here
         silently miscalibrates every price and Greek for that market type.
      2. divide by sqrt(reference_horizon_in_hours) to annualize (here:
         "hour-ize") it, the same Brownian scaling equity vol uses.
    reference_horizon_minutes is "how far out the value was effectively
    locked in" for this data — a calibration knob each MarketType chooses,
    not a fact derivable from the data itself.
    """
    std = float(np.std(historical_values))
    fractional_std = std / reference_level
    ref_hours = reference_horizon_minutes / 60.0
    return fractional_std / np.sqrt(ref_hours)
