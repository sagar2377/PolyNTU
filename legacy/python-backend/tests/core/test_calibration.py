import numpy as np
from core.calibration import calibrate_sigma


def test_calibrate_sigma_is_positive_and_scales_with_spread():
    rng = np.random.default_rng(0)
    tight = rng.normal(50, 3, size=1000)
    wide = rng.normal(50, 15, size=1000)
    sigma_tight = calibrate_sigma(tight, reference_horizon_minutes=10.0, reference_level=60.0)
    sigma_wide = calibrate_sigma(wide, reference_horizon_minutes=10.0, reference_level=60.0)
    assert sigma_tight > 0 and sigma_wide > 0
    assert sigma_wide > sigma_tight


def test_calibrate_sigma_scales_inversely_with_reference_level():
    rng = np.random.default_rng(0)
    values = rng.normal(50, 10, size=1000)
    sigma_small_ref = calibrate_sigma(values, reference_horizon_minutes=10.0, reference_level=10.0)
    sigma_large_ref = calibrate_sigma(values, reference_horizon_minutes=10.0, reference_level=100.0)
    assert sigma_small_ref > sigma_large_ref
