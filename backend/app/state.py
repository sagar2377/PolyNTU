"""
In-memory simulation state for the demo API: today's generated
MarketInstances per market, plus a simulated clock. Instances are
intentionally not persisted — regenerated deterministically (seeded RNG) on
startup, since they're simulator OUTPUT (see core/market.py's
MarketInstance docstring). Markets themselves (the templates) and
historical observations ARE persisted via core/db.py's Store, since those
represent durable config/logs a real deployment would keep.

The clock is an unbounded elapsed-minutes counter rather than a same-day
0-1440 counter: a shuttle route resolves many times a day, but an election
campaign spans many days, so the clock needs to run past a single day.
Shuttle's hour-of-day bucketing still works fine on an unbounded counter
since it only ever does `(minute // 60) % 24`.

A single in-process instance is fine for a university project demo; it is
not meant to survive a restart or run behind multiple workers.
"""
from typing import Dict, List

import numpy as np

from core.market import Market, MarketInstance, MarketType

EPOCH_START_MINUTE = 7 * 60  # "day 0, 7:00am" — matches the shuttle service window


class PlatformState:
    def __init__(self, seed: int = 42):
        self.rng = np.random.default_rng(seed)
        self.clock_minute = EPOCH_START_MINUTE
        self.instances: Dict[str, List[MarketInstance]] = {}

    def generate_instances_for(self, market: Market, market_type: MarketType) -> None:
        self.instances[market.id] = market_type.generate_instances(market, self.rng)

    def instances_for(self, market_id: str) -> List[MarketInstance]:
        return self.instances.get(market_id, [])

    def instance(self, market_id: str, instance_id: str) -> MarketInstance:
        for inst in self.instances_for(market_id):
            if inst.id == instance_id:
                return inst
        raise KeyError(f"no instance {instance_id} for market {market_id}")

    def minutes_remaining(self, resolution_minute: int) -> float:
        return float(resolution_minute - self.clock_minute)
