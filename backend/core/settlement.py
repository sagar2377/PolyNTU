"""
Generic settlement state machine for threshold contracts, usable by any
MarketType.

    OPEN --(resolution event recorded)--> RESOLVING --(settle)--> SETTLED
       \\--(resolution time passes, no event)--> EXPIRED

This module is pure business logic — it knows nothing about FastAPI or
SQLite, and it never imports a concrete MarketType (it's handed one as an
argument wherever it needs domain-specific payoff logic). db.py persists
Contract objects produced here; app/main.py wires the HTTP layer around
both.
"""
from dataclasses import dataclass, field
from datetime import datetime, timezone
from enum import Enum
from typing import Optional
import uuid

from . import pricing
from .market import Market, MarketInstance, MarketType


class ContractState(str, Enum):
    OPEN = "open"
    RESOLVING = "resolving"
    SETTLED = "settled"
    EXPIRED = "expired"


class InvalidTransition(Exception):
    pass


@dataclass
class Contract:
    id: str
    market_id: str
    instance_id: str
    contract_type: str              # 'call' or 'put'
    threshold: float
    resolution_minute: int
    spot_at_creation: float
    sigma_at_creation: float
    price_at_creation: float
    state: ContractState = ContractState.OPEN
    created_at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    realized_value: Optional[float] = None
    resolved_at: Optional[datetime] = None
    settlement_price: Optional[float] = None
    in_the_money: Optional[bool] = None
    settled_at: Optional[datetime] = None


def create_contract(market: Market, market_type: MarketType, instance: MarketInstance,
                     contract_type: str, threshold: float, spot: float, sigma: float,
                     minutes_remaining: float) -> Contract:
    quote = pricing.price_threshold_contract(
        spot, threshold, minutes_remaining, sigma, contract_type,
        to_underlying=market_type.to_underlying,
        payoff_fn=(market_type.payoff_call if contract_type == 'call' else market_type.payoff_put),
        delta_sign=market_type.delta_sign,
    )
    return Contract(
        id=str(uuid.uuid4()),
        market_id=market.id,
        instance_id=instance.id,
        contract_type=contract_type,
        threshold=threshold,
        resolution_minute=instance.resolution_minute,
        spot_at_creation=spot,
        sigma_at_creation=sigma,
        price_at_creation=quote.price,
    )


def record_resolution_event(contract: Contract, realized_value: float) -> Contract:
    """The simulated 'this instance resolved' event. Only legal from OPEN."""
    if contract.state != ContractState.OPEN:
        raise InvalidTransition(f"cannot record resolution from state {contract.state}")
    contract.realized_value = realized_value
    contract.resolved_at = datetime.now(timezone.utc)
    contract.state = ContractState.RESOLVING
    return contract


def settle(contract: Contract, market_type: MarketType) -> Contract:
    """Auto-settlement: compute the realized payoff and record the outcome."""
    if contract.state != ContractState.RESOLVING:
        raise InvalidTransition(f"cannot settle from state {contract.state}")
    payoff_fn = market_type.payoff_call if contract.contract_type == 'call' else market_type.payoff_put
    payoff = payoff_fn(contract.realized_value, contract.threshold)
    contract.settlement_price = payoff
    contract.in_the_money = payoff > 0
    contract.settled_at = datetime.now(timezone.utc)
    contract.state = ContractState.SETTLED
    return contract


def expire(contract: Contract) -> Contract:
    """For a contract whose resolution time has passed with no event recorded."""
    if contract.state not in (ContractState.OPEN, ContractState.RESOLVING):
        raise InvalidTransition(f"cannot expire from state {contract.state}")
    contract.state = ContractState.EXPIRED
    return contract
