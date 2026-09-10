"""
Market-type-agnostic abstraction: Market, MarketInstance, and the
MarketType plugin interface. core/pricing.py and core/settlement.py never
import a concrete MarketType — they only call methods on this interface.
"""
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any


@dataclass
class Market:
    """
    A market TEMPLATE — e.g. "NTU Blue Line shuttle arrivals" or "Demo Hall
    Committee Election." Generates one or more resolvable MarketInstances
    over time. Persisted in the DB (core/db.py).
    """
    id: str
    market_type: str            # key into the MARKET_TYPES registry
    title: str
    resolution_criterion: str   # published, human-readable; must be non-empty
    unit_label: str             # "minutes of delay", "% vote share", ... — used by the UI
    params: dict[str, Any] = field(default_factory=dict)
    is_active: bool = True


@dataclass
class MarketInstance:
    """
    One concrete resolvable occurrence of a Market — e.g. the 7:30am trip,
    or a specific fictional election's campaign. NOT persisted: it's
    simulator output, regenerated deterministically from a seeded RNG each
    time the platform starts (same rationale the original ShuttlePredict
    schedule generator used).
    """
    id: str                      # f"{market_id}:{resolution_minute}"
    market_id: str
    resolution_minute: int       # absolute simulated minutes since the platform epoch
    truth_seed: dict[str, Any]   # ground truth needed to resolve later, market-type-specific shape


class MarketType(ABC):
    """
    Everything a market type must supply. A MarketType instance is
    stateless with respect to any particular Market/MarketInstance — all
    the data it needs is passed in as arguments.
    """
    key: str
    contract_kind: str = "threshold"   # the only kind built so far; "binary" is future work

    # dUnderlying/dValue for this type's to_underlying transform: +1 if it's a
    # plain (optionally shifted) identity, -1 if it negates. Because
    # to_underlying is always affine in this codebase, this single constant
    # is all core/pricing.py needs to correct delta for the transform's
    # direction — gamma is invariant to the sign (it's a second derivative),
    # and vega/theta/rho don't depend on the value-transform at all.
    delta_sign: float = 1.0

    @abstractmethod
    def generate_instances(self, market: Market, rng) -> list[MarketInstance]:
        """Simulate this market's schedule of resolvable instances."""

    @abstractmethod
    def live_spot(self, market: Market, instance: MarketInstance, now_minute: int,
                   rng, observations: list[float]) -> tuple[float, float]:
        """
        Returns (current estimated domain value, calibrated sigma) for this
        instance at `now_minute`. `observations` is the market's logged
        historical values for whatever calibration bucket this instance
        falls into (see Store.observations in core/db.py).
        """

    @abstractmethod
    def resolve(self, market: Market, instance: MarketInstance) -> float:
        """Returns the realized domain value used for settlement payoff."""

    @abstractmethod
    def to_underlying(self, value: float) -> float:
        """Transform a domain value into a strictly-positive BS-safe underlying."""

    @abstractmethod
    def payoff_call(self, realized: float, threshold: float) -> float: ...

    @abstractmethod
    def payoff_put(self, realized: float, threshold: float) -> float: ...

    def contract_label(self, contract_type: str) -> str:
        """Human label for a contract type. Override per market type."""
        return "above threshold" if contract_type == "call" else "below threshold"

    def greek_hints(self) -> dict[str, str]:
        """UI hint text per Greek, domain-flavored. Override per market type."""
        return {
            "delta": "sensitivity to the current live estimate moving",
            "gamma": "sensitivity of delta itself to the live estimate",
            "vega": "sensitivity to this market's calibrated volatility",
            "theta": "value decay as the resolution time approaches",
            "rho": "not meaningful here (rate is fixed at 0)",
        }

    def validate_params(self, params: dict) -> list[str]:
        """Return validation error strings (empty = valid). Used by admin market creation."""
        return []

    def calibration_bucket(self, resolution_minute: int) -> str:
        """Which historical-observation bucket an instance falls into (e.g. hour-of-day)."""
        return "default"

    def seed_demo_history(self, market: Market, rng, log_observation) -> None:
        """
        Optional: populate demo historical observations via log_observation(bucket, value).
        No-op by default — real deployments would rely on accumulating real
        logs instead of seeding fake history.
        """
        return None


MARKET_TYPES: dict[str, MarketType] = {}


def register(market_type: MarketType) -> None:
    MARKET_TYPES[market_type.key] = market_type


def get(key: str) -> MarketType:
    return MARKET_TYPES[key]
