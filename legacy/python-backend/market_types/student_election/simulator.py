"""
Fictional vote-share simulator (Student Election market type).

FICTIONAL DEMO DATA ONLY. Candidate names, org names, and all vote/polling
data here are synthetic, generated purely to demonstrate the MarketType
abstraction — none of it is connected to, derived from, or intended to
represent any real NTU election, organization, or person. See
market_type.py's validate_params, which refuses to create a market of this
type unless params["is_fictional"] is True.

MODELING NOTE: unlike shuttle delay (bounded, right-skewed, resolves every
few minutes all day), a vote share is naturally bounded to [0, 100] and a
single market resolves only once, at the end of a multi-day campaign. See
market_type.py's to_underlying — no shift+negate trick is needed here since
vote share is already non-negative, unlike shuttle delay.
"""
import numpy as np

MIN_VOTE_SHARE_PCT = 0.0
MAX_VOTE_SHARE_PCT = 100.0


def sample_final_vote_share_pct(n: int, mean_pct: float = 50.0, spread_pct: float = 12.0,
                                 rng=None) -> np.ndarray:
    """Draw n plausible final vote-share outcomes for one candidate in a two-way fictional race."""
    rng = rng or np.random.default_rng()
    raw = rng.normal(mean_pct, spread_pct, size=n)
    return np.clip(raw, MIN_VOTE_SHARE_PCT, MAX_VOTE_SHARE_PCT)


def generate_past_election_results(n_elections: int = 60, mean_pct: float = 50.0,
                                    spread_pct: float = 12.0, rng=None) -> np.ndarray:
    """
    Stand-in for a historical log of past fictional demo elections' final
    results, used to calibrate this market type's volatility. Purely
    synthetic — there is no real election data anywhere in this project.
    """
    return sample_final_vote_share_pct(n_elections, mean_pct, spread_pct, rng)
