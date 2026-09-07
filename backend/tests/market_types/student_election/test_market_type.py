import numpy as np
from core import pricing
from core.market import Market
from market_types.student_election.market_type import StudentElectionMarketType

MT = StudentElectionMarketType()

FICTIONAL_MARKET = Market(
    id="election-1", market_type="student_election",
    title="Demo Hall Committee Election — Candidate A (fictional demo)",
    resolution_criterion="Settles against the simulated final vote count for this fictional demo election.",
    unit_label="% vote share",
    params={"is_fictional": True, "candidate": "Candidate A", "mean_pct": 50.0,
            "spread_pct": 12.0, "campaign_length_minutes": 7 * 24 * 60},
)


def quote(spot, threshold, minutes_remaining, sigma, contract_type):
    payoff_fn = MT.payoff_call if contract_type == 'call' else MT.payoff_put
    return pricing.price_threshold_contract(
        spot, threshold, minutes_remaining, sigma, contract_type,
        to_underlying=MT.to_underlying, payoff_fn=payoff_fn, delta_sign=MT.delta_sign)


def test_validate_params_requires_is_fictional_true():
    assert MT.validate_params({"candidate": "Candidate A"}) != []
    assert MT.validate_params({"candidate": "Candidate A", "is_fictional": False}) != []
    assert MT.validate_params({"candidate": "Candidate A", "is_fictional": True}) == []


def test_validate_params_requires_candidate():
    errors = MT.validate_params({"is_fictional": True})
    assert any("candidate" in e for e in errors)


def test_generate_instances_produces_exactly_one_resolution_event():
    rng = np.random.default_rng(0)
    instances = MT.generate_instances(FICTIONAL_MARKET, rng)
    assert len(instances) == 1
    assert instances[0].resolution_minute == FICTIONAL_MARKET.params["campaign_length_minutes"]
    assert "final_vote_share_pct" in instances[0].truth_seed


def test_resolve_returns_the_seeded_final_result():
    rng = np.random.default_rng(0)
    instance = MT.generate_instances(FICTIONAL_MARKET, rng)[0]
    assert MT.resolve(FICTIONAL_MARKET, instance) == instance.truth_seed["final_vote_share_pct"]


def test_live_spot_converges_to_true_value_near_resolution():
    rng = np.random.default_rng(0)
    instance = MT.generate_instances(FICTIONAL_MARKET, rng)[0]
    true_pct = instance.truth_seed["final_vote_share_pct"]

    far_estimates = [MT.live_spot(FICTIONAL_MARKET, instance, now_minute=0, rng=rng, observations=[])[0]
                      for _ in range(200)]
    near_estimates = [MT.live_spot(FICTIONAL_MARKET, instance,
                                    now_minute=instance.resolution_minute - 1, rng=rng, observations=[])[0]
                       for _ in range(200)]
    assert np.std(far_estimates) > np.std(near_estimates)
    assert abs(np.mean(near_estimates) - true_pct) < 5.0


def test_call_worth_more_when_comfortably_above_threshold_opposite_sign_from_shuttle():
    """Election uses delta_sign=+1 (no negation), the opposite of shuttle's -1."""
    near_miss = quote(48.0, 50.0, 20, 0.2, 'call')
    comfortable = quote(65.0, 50.0, 20, 0.2, 'call')
    assert comfortable.price > near_miss.price
    assert comfortable.delta > 0  # rising vote share HELPS this call (opposite of shuttle)


def test_contract_labels():
    assert MT.contract_label('call') == "exceeds threshold vote share"
    assert MT.contract_label('put') == "falls short of threshold vote share"
