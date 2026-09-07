import numpy as np
from app import simulator


def test_sampled_delays_respect_bounds():
    rng = np.random.default_rng(0)
    delays = simulator.sample_delay_minutes(hour_of_day=8, n=5000, rng=rng)
    assert delays.min() >= simulator.MIN_DELAY_MINUTES
    assert delays.max() <= simulator.MAX_DELAY_MINUTES


def test_peak_hours_have_higher_mean_and_variance_than_offpeak():
    rng = np.random.default_rng(0)
    peak = simulator.sample_delay_minutes(hour_of_day=8, n=20000, rng=rng)
    offpeak = simulator.sample_delay_minutes(hour_of_day=14, n=20000, rng=rng)
    assert peak.mean() > offpeak.mean()
    assert peak.std() > offpeak.std()


def test_daily_schedule_is_well_formed():
    rng = np.random.default_rng(0)
    trips = simulator.generate_daily_schedule('R1', headway_minutes=15, rng=rng)
    assert len(trips) > 0
    minutes = [t.scheduled_minute_of_day for t in trips]
    assert minutes == sorted(minutes)
    assert all(isinstance(t.true_delay_minutes, float) for t in trips)


def test_predicted_delay_converges_to_true_delay_near_arrival():
    rng = np.random.default_rng(0)
    true_delay = 4.0
    far_out = [simulator.predicted_delay_minutes(true_delay, 60, 60, prediction_sigma=6.0, rng=rng)
               for _ in range(500)]
    near_arrival = [simulator.predicted_delay_minutes(true_delay, 0.5, 60, prediction_sigma=6.0, rng=rng)
                    for _ in range(500)]
    assert np.std(far_out) > np.std(near_arrival)
    assert simulator.predicted_delay_minutes(true_delay, 0, 60, prediction_sigma=6.0) == true_delay


def test_calibrate_sigma_is_positive_and_scales_with_spread():
    rng = np.random.default_rng(0)
    tight = simulator.sample_delay_minutes(hour_of_day=14, n=1000, rng=rng)
    wide = simulator.sample_delay_minutes(hour_of_day=8, n=1000, rng=rng)
    sigma_tight = simulator.calibrate_sigma(tight)
    sigma_wide = simulator.calibrate_sigma(wide)
    assert sigma_tight > 0 and sigma_wide > 0
    assert sigma_wide > sigma_tight
