"""
Student Election market type — FICTIONAL DEMO ONLY.

Domain mapping:
    Spot (S)      -> current live-estimated vote-share percentage for a tracked candidate
    Strike (K)    -> a vote-share threshold being evaluated (e.g. "exceeds 50%")
    Volatility    -> historical variance of final results across past fictional demo elections
    "Call"        -> confidence contract that the candidate's final vote share EXCEEDS the threshold
    "Put"         -> confidence contract that it falls SHORT of the threshold

Unlike shuttle delay, vote share is already non-negative, so to_underlying
is a plain positivity floor (no negation) — delta_sign = +1.0. This is the
deliberate stress-test of the abstraction: the same core pricing/settlement
code handles both a negated transform (shuttle) and a non-negated one
(here) with no core-level change.

SAFETY: validate_params refuses to create a market of this type unless
params["is_fictional"] is True, and the demo instance this project seeds
uses placeholder names ("Candidate A"/"Candidate B", "Demo Hall Committee
Election") with no resemblance to any real NTU race. This is deliberate: a
market that prices a real, in-progress election's real candidates would
expose real people to a public "confidence score" without consent, and
risks influencing the very vote it claims to just forecast. Supporting real
elections responsibly (consent workflows, published methodology,
independent governance) is out of scope for this project.
"""
import numpy as np

from core.calibration import calibrate_sigma
from core.live_estimate import brownian_bridge_estimate
from core.market import Market, MarketInstance, MarketType

from . import simulator

EPSILON = 0.01  # positivity floor; vote share is already >= 0, no negation needed


class StudentElectionMarketType(MarketType):
    key = "student_election"
    contract_kind = "threshold"
    delta_sign = 1.0

    def to_underlying(self, vote_share_pct: float) -> float:
        return max(vote_share_pct, 0.0) + EPSILON

    def payoff_call(self, realized_pct: float, threshold: float) -> float:
        return max(realized_pct - threshold, 0.0)

    def payoff_put(self, realized_pct: float, threshold: float) -> float:
        return max(threshold - realized_pct, 0.0)

    def contract_label(self, contract_type: str) -> str:
        return ("exceeds threshold vote share" if contract_type == "call"
                else "falls short of threshold vote share")

    def greek_hints(self) -> dict[str, str]:
        return {
            "delta": "sensitivity to the candidate's current estimated vote share moving",
            "gamma": "sensitivity of delta itself to the estimated vote share",
            "vega": "sensitivity to this race's calibrated polling volatility",
            "theta": "value decay as election night approaches",
            "rho": "not meaningful here (rate is fixed at 0)",
        }

    def validate_params(self, params: dict) -> list[str]:
        errors = []
        if params.get("is_fictional") is not True:
            errors.append(
                "student_election markets require params.is_fictional = true; "
                "this market type does not support real elections (see module docstring)")
        if "candidate" not in params:
            errors.append("student_election markets require params.candidate")
        return errors

    def calibration_bucket(self, resolution_minute: int) -> str:
        return "election"  # one calibration pool per market; no natural hour-of-day bucketing

    def generate_instances(self, market: Market, rng) -> list[MarketInstance]:
        campaign_length_minutes = market.params.get("campaign_length_minutes", 7 * 24 * 60)
        final_pct = float(simulator.sample_final_vote_share_pct(
            1, mean_pct=market.params.get("mean_pct", 50.0),
            spread_pct=market.params.get("spread_pct", 12.0), rng=rng)[0])
        return [MarketInstance(
            id=f"{market.id}:{campaign_length_minutes}",
            market_id=market.id,
            resolution_minute=campaign_length_minutes,
            truth_seed={"final_vote_share_pct": final_pct},
        )]

    def live_spot(self, market: Market, instance: MarketInstance, now_minute: int,
                   rng, observations: list[float]) -> tuple[float, float]:
        minutes_remaining = max(instance.resolution_minute - now_minute, 0)
        reference_level = market.params.get("mean_pct", 50.0)
        if len(observations) >= 10:
            sigma = calibrate_sigma(np.asarray(observations), reference_horizon_minutes=24 * 60,
                                     reference_level=reference_level)
        else:
            sigma = 0.3
        # No cap on how far the noise scaling reaches back (unlike shuttle's
        # 60-minute cap): a poll taken early in a multi-day campaign really
        # is much noisier relative to the final result than one taken the
        # night before, so letting noise grow across the whole campaign is
        # intentional here, not an oversight.
        predicted = brownian_bridge_estimate(
            instance.truth_seed["final_vote_share_pct"], minutes_remaining,
            minutes_to_horizon=instance.resolution_minute,
            sigma_abs=sigma * reference_level, rng=rng)
        return float(np.clip(predicted, 0.0, 100.0)), sigma

    def resolve(self, market: Market, instance: MarketInstance) -> float:
        return instance.truth_seed["final_vote_share_pct"]

    def seed_demo_history(self, market: Market, rng, log_observation) -> None:
        results = simulator.generate_past_election_results(
            n_elections=60, mean_pct=market.params.get("mean_pct", 50.0),
            spread_pct=market.params.get("spread_pct", 12.0), rng=rng)
        for r in results:
            log_observation("election", float(r))
