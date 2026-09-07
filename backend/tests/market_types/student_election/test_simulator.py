import numpy as np
from market_types.student_election import simulator


def test_sampled_vote_shares_respect_bounds():
    rng = np.random.default_rng(0)
    shares = simulator.sample_final_vote_share_pct(5000, mean_pct=50.0, spread_pct=20.0, rng=rng)
    assert shares.min() >= simulator.MIN_VOTE_SHARE_PCT
    assert shares.max() <= simulator.MAX_VOTE_SHARE_PCT


def test_higher_spread_produces_higher_variance():
    rng = np.random.default_rng(0)
    tight = simulator.sample_final_vote_share_pct(5000, mean_pct=50.0, spread_pct=3.0, rng=rng)
    wide = simulator.sample_final_vote_share_pct(5000, mean_pct=50.0, spread_pct=20.0, rng=rng)
    assert wide.std() > tight.std()


def test_past_election_results_same_shape_as_single_sample():
    rng = np.random.default_rng(0)
    results = simulator.generate_past_election_results(n_elections=60, rng=rng)
    assert len(results) == 60
