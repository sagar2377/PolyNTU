"""
In-memory simulation state for the demo API: today's generated schedule per
route, plus a simulated clock. This is intentionally not persisted — it's
regenerated deterministically (seeded RNG) on startup, since it's simulator
OUTPUT, not the kind of state that needs a durable record. Historical delay
data (used to calibrate volatility) IS written to the DB in db.py, since
that represents an accumulating log a real deployment would query.

A single in-process instance is fine for a university project demo; it is
not meant to survive a restart or run behind multiple workers.
"""
from typing import Dict, List

import numpy as np

from . import simulator

ROUTES = ["NTU-blue", "NTU-red"]
SERVICE_START_MINUTE = 7 * 60
SERVICE_END_MINUTE = 22 * 60
CALIBRATION_HOURS = list(range(SERVICE_START_MINUTE // 60, SERVICE_END_MINUTE // 60 + 1))


class SimulationState:
    def __init__(self, seed: int = 42):
        self.rng = np.random.default_rng(seed)
        self.clock_minute = SERVICE_START_MINUTE
        self.schedules: Dict[str, List[simulator.ScheduledTrip]] = {}
        for route_id in ROUTES:
            self.schedules[route_id] = simulator.generate_daily_schedule(
                route_id, service_start_minute=SERVICE_START_MINUTE,
                service_end_minute=SERVICE_END_MINUTE, rng=self.rng)

    def trip(self, route_id: str, scheduled_minute_of_day: int) -> simulator.ScheduledTrip:
        for t in self.schedules[route_id]:
            if t.scheduled_minute_of_day == scheduled_minute_of_day:
                return t
        raise KeyError(f"no trip at minute {scheduled_minute_of_day} for {route_id}")

    def minutes_remaining(self, scheduled_minute_of_day: int) -> float:
        return float(scheduled_minute_of_day - self.clock_minute)

    def seed_historical_data(self, store, days_per_hour: int = 60) -> None:
        """Populate the DB with a simulated history for sigma calibration."""
        for route_id in ROUTES:
            for hour in CALIBRATION_HOURS:
                delays = simulator.generate_historical_delays(
                    route_id, hour, n_days=days_per_hour, rng=self.rng)
                for d in delays:
                    store.log_historical_delay(route_id, hour, float(d))
