"""
Arrival data simulator (Shuttle Arrival market type).

There is no public NTU shuttle API, so this module stands in for one. It
generates a bus schedule and, for each scheduled trip, a realistic arrival
delay in minutes.

MODELING LIMITATION (read before reusing this for real data):
Bus arrivals are bounded and schedule-anchored, not a free random walk:
- A bus essentially can't arrive much before its scheduled time (drivers
  hold at stops), so delay has a hard floor.
- Delay is right-skewed: most trips are on time or slightly late, a few are
  very late (traffic, breakdowns), almost none are very early.
GBM (used by the pricing engine's Monte Carlo cross-check) assumes returns
are unbounded and lognormal in both directions, which does not hold here.
We therefore generate the *underlying* delay data from a shifted Gamma
distribution (right-skewed, bounded below) rather than GBM, and only bring
GBM back in at the pricing layer as a tractable approximation. Swapping this
module's `sample_delay_minutes` for a real bootstrap resample of logged
historical delays (once such logs exist) is the intended upgrade path —
core.calibration.calibrate_sigma is written to consume either source
identically, since both just produce an array of minute-delays.

The generic "noisy live estimate converging to a true value" and "absolute
stdev -> fractional BS sigma" logic used to live here but had no
shuttle-specific content — see core/live_estimate.py and
core/calibration.py, used by market_type.py.
"""
from dataclasses import dataclass
from typing import List
import numpy as np

MIN_DELAY_MINUTES = -3.0
MAX_DELAY_MINUTES = 25.0

# Shifted-Gamma parameters per time-of-day bucket. Peak hours have both a
# higher mean delay and more spread, matching real campus shuttle behavior.
HOURLY_PROFILES = {
    "peak": {"shape": 2.2, "scale": 3.0, "shift": 2.5},
    "offpeak": {"shape": 1.4, "scale": 1.6, "shift": 1.0},
}
PEAK_HOURS = {7, 8, 9, 12, 17, 18}


def hour_bucket(hour_of_day: int) -> str:
    return "peak" if hour_of_day in PEAK_HOURS else "offpeak"


def sample_delay_minutes(hour_of_day: int, n: int, rng=None) -> np.ndarray:
    """Draw n plausible true arrival delays (minutes) for the given hour."""
    rng = rng or np.random.default_rng()
    profile = HOURLY_PROFILES[hour_bucket(hour_of_day)]
    raw = rng.gamma(profile["shape"], profile["scale"], size=n) - profile["shift"]
    return np.clip(raw, MIN_DELAY_MINUTES, MAX_DELAY_MINUTES)


@dataclass
class ScheduledTrip:
    route_id: str
    scheduled_minute_of_day: int
    true_delay_minutes: float


def generate_daily_schedule(route_id: str, headway_minutes: int = 15,
                             service_start_minute: int = 7 * 60,
                             service_end_minute: int = 22 * 60,
                             rng=None) -> List[ScheduledTrip]:
    """One day's worth of scheduled trips for a route, each with a true delay."""
    rng = rng or np.random.default_rng()
    minutes = list(range(service_start_minute, service_end_minute, headway_minutes))
    trips = []
    for m in minutes:
        hour = (m // 60) % 24
        delay = float(sample_delay_minutes(hour, 1, rng)[0])
        trips.append(ScheduledTrip(route_id, m, delay))
    return trips


def generate_historical_delays(route_id: str, hour_of_day: int, n_days: int = 90,
                                rng=None) -> np.ndarray:
    """
    Stand-in for a historical log query: n_days worth of observed delays for
    a route at a given hour, used to calibrate volatility. Once real logs
    exist, replace this call with a DB read of actual past delays.
    """
    return sample_delay_minutes(hour_of_day, n_days, rng)
