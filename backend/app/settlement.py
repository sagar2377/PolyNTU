"""
Settlement state machine for arrival-window confidence contracts.

    CREATED --(bus arrival event recorded)--> ARRIVED --(settle)--> SETTLED
       \\--(scheduled time passes, no arrival event)--> EXPIRED

This module is pure business logic — it knows nothing about FastAPI or
SQLite. db.py persists Contract objects produced here; main.py wires the
HTTP layer around both.
"""
from dataclasses import dataclass, field
from datetime import datetime, timezone
from enum import Enum
from typing import Optional
import uuid

from . import pricing


class ContractState(str, Enum):
    CREATED = "created"
    ARRIVED = "arrived"
    SETTLED = "settled"
    EXPIRED = "expired"


class InvalidTransition(Exception):
    pass


@dataclass
class Contract:
    id: str
    route_id: str
    contract_type: str              # 'call' or 'put'
    threshold_minutes: float
    scheduled_minute_of_day: int
    predicted_delay_at_creation: float
    sigma_at_creation: float
    price_at_creation: float
    state: ContractState = ContractState.CREATED
    created_at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    actual_delay_minutes: Optional[float] = None
    arrived_at: Optional[datetime] = None
    settlement_price: Optional[float] = None
    in_the_money: Optional[bool] = None
    settled_at: Optional[datetime] = None


def create_contract(route_id: str, contract_type: str, threshold_minutes: float,
                     scheduled_minute_of_day: int, predicted_delay_minutes: float,
                     minutes_remaining: float, sigma: float) -> Contract:
    quote = pricing.price_confidence_contract(
        predicted_delay_minutes, threshold_minutes, minutes_remaining, sigma, contract_type)
    return Contract(
        id=str(uuid.uuid4()),
        route_id=route_id,
        contract_type=contract_type,
        threshold_minutes=threshold_minutes,
        scheduled_minute_of_day=scheduled_minute_of_day,
        predicted_delay_at_creation=predicted_delay_minutes,
        sigma_at_creation=sigma,
        price_at_creation=quote.price,
    )


def record_arrival(contract: Contract, actual_delay_minutes: float) -> Contract:
    """The simulated 'bus arrives' event. Only legal from CREATED."""
    if contract.state != ContractState.CREATED:
        raise InvalidTransition(f"cannot record arrival from state {contract.state}")
    contract.actual_delay_minutes = actual_delay_minutes
    contract.arrived_at = datetime.now(timezone.utc)
    contract.state = ContractState.ARRIVED
    return contract


def settle(contract: Contract) -> Contract:
    """Auto-settlement: compute the realized payoff and record the outcome."""
    if contract.state != ContractState.ARRIVED:
        raise InvalidTransition(f"cannot settle from state {contract.state}")
    payoff_fn = pricing.payoff_call if contract.contract_type == 'call' else pricing.payoff_put
    payoff = payoff_fn(contract.actual_delay_minutes, contract.threshold_minutes)
    contract.settlement_price = payoff
    contract.in_the_money = payoff > 0
    contract.settled_at = datetime.now(timezone.utc)
    contract.state = ContractState.SETTLED
    return contract


def expire(contract: Contract) -> Contract:
    """For a contract whose scheduled time has passed with no arrival event recorded."""
    if contract.state not in (ContractState.CREATED, ContractState.ARRIVED):
        raise InvalidTransition(f"cannot expire from state {contract.state}")
    contract.state = ContractState.EXPIRED
    return contract
