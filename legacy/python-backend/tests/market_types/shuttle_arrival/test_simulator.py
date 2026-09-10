import numpy as np
from market_types.shuttle_arrival import simulator


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


def test_generate_historical_delays_respects_bounds():
    rng = np.random.default_rng(0)
    delays = simulator.generate_historical_delays('R1', hour_of_day=8, n_days=200, rng=rng)
    assert len(delays) == 200
    assert delays.min() >= simulator.MIN_DELAY_MINUTES
    assert delays.max() <= simulator.MAX_DELAY_MINUTES
