import numpy as np
from core.live_estimate import brownian_bridge_estimate


def test_predicted_value_converges_to_true_value_near_resolution():
    rng = np.random.default_rng(0)
    true_value = 4.0
    far_out = [brownian_bridge_estimate(true_value, 60, 60, sigma_abs=6.0, rng=rng) for _ in range(500)]
    near_resolution = [brownian_bridge_estimate(true_value, 0.5, 60, sigma_abs=6.0, rng=rng) for _ in range(500)]
    assert np.std(far_out) > np.std(near_resolution)
    assert brownian_bridge_estimate(true_value, 0, 60, sigma_abs=6.0) == true_value


def test_estimate_scales_with_sigma():
    rng = np.random.default_rng(0)
    low_noise = [brownian_bridge_estimate(0.0, 30, 60, sigma_abs=1.0, rng=rng) for _ in range(500)]
    high_noise = [brownian_bridge_estimate(0.0, 30, 60, sigma_abs=10.0, rng=rng) for _ in range(500)]
    assert np.std(high_noise) > np.std(low_noise)
