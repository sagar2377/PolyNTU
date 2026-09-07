"""
PolyNTU API.

Purpose: an academic prediction-market platform for the NTU campus,
pricing "confidence contracts" on uncertain campus outcomes using
derivatives-pricing theory (Black-Scholes, Greeks, Monte Carlo). This is an
educational SC2006 project — every price is a simulated confidence unit,
never real currency, and there is no betting/wagering framing anywhere in
this API (see README).

ShuttlePredict (shuttle arrival delay) is the first Market Type; a
fictional demo student-election market proves the abstraction generalizes
to a market with an opposite sign convention and a single, multi-day
resolution event instead of many short recurring ones. See
market_types/student_election/market_type.py's module docstring for why
that market type refuses to be instantiated against a real election.
"""
import os
import uuid
from typing import List, Optional

import numpy as np
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel

import market_types  # noqa: F401 — side effect: registers built-in market types
from core import db, pricing, settlement
from core import market as market_registry
from core.market import Market

from . import state

app = FastAPI(title="PolyNTU API")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_methods=["*"], allow_headers=["*"],
)

store: db.Store
platform: state.PlatformState

DEFAULT_MARKETS = [
    Market(
        id="shuttle-ntu-blue", market_type="shuttle_arrival",
        title="NTU Blue Line Shuttle Arrival",
        resolution_criterion=(
            "Settles against the simulated GPS arrival event for each scheduled trip "
            "(see market_types/shuttle_arrival for the simulator standing in for a real feed)."),
        unit_label="minutes of delay",
        params={"route_id": "NTU-blue"},
    ),
    Market(
        id="shuttle-ntu-red", market_type="shuttle_arrival",
        title="NTU Red Line Shuttle Arrival",
        resolution_criterion=(
            "Settles against the simulated GPS arrival event for each scheduled trip "
            "(see market_types/shuttle_arrival for the simulator standing in for a real feed)."),
        unit_label="minutes of delay",
        params={"route_id": "NTU-red"},
    ),
    Market(
        id="demo-hall-election-candidate-a", market_type="student_election",
        title="Demo Hall Committee Election — Candidate A (fictional demo)",
        resolution_criterion=(
            "Settles against the simulated final vote count for this fictional demo "
            "election; not connected to any real NTU election, organization, or person."),
        unit_label="% vote share",
        params={"is_fictional": True, "candidate": "Candidate A",
                "mean_pct": 54.0, "spread_pct": 10.0, "campaign_length_minutes": 7 * 24 * 60},
    ),
]


def configure(db_url: str | None = None, seed: int = 42) -> None:
    """(Re)builds the module's store/platform instances. Callable again by tests for isolation."""
    global store, platform
    db_url = db_url or os.environ.get("POLYNTU_DB_URL", "sqlite:///polyntu.db")
    store = db.Store(db.make_session_factory(db_url))
    platform = state.PlatformState(seed=seed)
    for spec in DEFAULT_MARKETS:
        market_type = market_registry.get(spec.market_type)
        existing = store.get_market(spec.id)
        if existing is None:
            errors = market_type.validate_params(spec.params)
            if errors:
                raise RuntimeError(f"default market {spec.id} failed validation: {errors}")
            store.save_market(spec)
            market_type.seed_demo_history(
                spec, platform.rng, lambda bucket, value, mid=spec.id: store.log_observation(mid, bucket, value))
            market = spec
        else:
            market = existing
        platform.generate_instances_for(market, market_type)


configure()


def _market_and_type(market_id: str):
    m = store.get_market(market_id)
    if m is None:
        raise HTTPException(404, "unknown market")
    return m, market_registry.get(m.market_type)


def _live_spot(market, market_type, instance):
    bucket = market_type.calibration_bucket(instance.resolution_minute)
    observations = store.observations(market.id, bucket)
    return market_type.live_spot(market, instance, platform.clock_minute, platform.rng, observations)


class MarketView(BaseModel):
    id: str
    market_type: str
    title: str
    resolution_criterion: str
    unit_label: str


class InstanceView(BaseModel):
    instance_id: str
    resolution_minute: int
    minutes_remaining: float
    spot: float


class GreeksView(BaseModel):
    delta: float
    gamma: float
    vega: float
    theta: float
    rho: float


class ContractQuoteView(BaseModel):
    contract_type: str
    label: str
    threshold: float
    price: float
    greeks: GreeksView
    is_intrinsic: bool


class QuoteResponse(BaseModel):
    market_id: str
    instance_id: str
    minutes_remaining: float
    spot: float
    sigma: float
    unit_label: str
    greek_hints: dict[str, str]
    contracts: List[ContractQuoteView]
    monte_carlo_gbm_price: float
    monte_carlo_bootstrap_price: float


class CreateContractRequest(BaseModel):
    market_id: str
    instance_id: str
    contract_type: str
    threshold: float


class ContractView(BaseModel):
    id: str
    market_id: str
    market_title: Optional[str] = None
    instance_id: str
    contract_type: str
    threshold: float
    resolution_minute: int
    spot_at_creation: float
    sigma_at_creation: float
    price_at_creation: float
    state: str
    realized_value: Optional[float]
    settlement_price: Optional[float]
    in_the_money: Optional[bool]


def _contract_view(c: settlement.Contract, market_title: Optional[str] = None) -> ContractView:
    return ContractView(
        id=c.id, market_id=c.market_id, market_title=market_title, instance_id=c.instance_id,
        contract_type=c.contract_type, threshold=c.threshold, resolution_minute=c.resolution_minute,
        spot_at_creation=c.spot_at_creation, sigma_at_creation=c.sigma_at_creation,
        price_at_creation=c.price_at_creation, state=c.state.value,
        realized_value=c.realized_value, settlement_price=c.settlement_price,
        in_the_money=c.in_the_money,
    )


@app.get("/health")
def health():
    return {"status": "ok"}


@app.get("/markets", response_model=List[MarketView])
def list_markets():
    return [MarketView(id=m.id, market_type=m.market_type, title=m.title,
                        resolution_criterion=m.resolution_criterion, unit_label=m.unit_label)
            for m in store.list_markets()]


@app.get("/markets/{market_id}/instances", response_model=List[InstanceView])
def get_instances(market_id: str):
    market, market_type = _market_and_type(market_id)
    instances = platform.instances_for(market_id)
    out = []
    for instance in instances:
        minutes_remaining = platform.minutes_remaining(instance.resolution_minute)
        if minutes_remaining < -30 and len(instances) > 1:
            continue  # drop long-past recurring instances (e.g. shuttle trips), keep the list demo-sized
        spot, _ = _live_spot(market, market_type, instance)
        out.append(InstanceView(instance_id=instance.id, resolution_minute=instance.resolution_minute,
                                 minutes_remaining=minutes_remaining, spot=spot))
    return out


@app.get("/markets/{market_id}/quote", response_model=QuoteResponse)
def get_quote(market_id: str, instance_id: str, thresholds: str = "0,5,10"):
    market, market_type = _market_and_type(market_id)
    try:
        instance = platform.instance(market_id, instance_id)
    except KeyError:
        raise HTTPException(404, "unknown instance")

    minutes_remaining = max(platform.minutes_remaining(instance.resolution_minute), 0.0001)
    spot, sigma = _live_spot(market, market_type, instance)

    threshold_list = [float(t) for t in thresholds.split(",")]
    contracts = []
    for threshold in threshold_list:
        for contract_type in ("call", "put"):
            payoff_fn = market_type.payoff_call if contract_type == "call" else market_type.payoff_put
            quote = pricing.price_threshold_contract(
                spot, threshold, minutes_remaining, sigma, contract_type,
                to_underlying=market_type.to_underlying, payoff_fn=payoff_fn,
                delta_sign=market_type.delta_sign)
            contracts.append(ContractQuoteView(
                contract_type=contract_type, label=market_type.contract_label(contract_type),
                threshold=threshold, price=quote.price,
                greeks=GreeksView(delta=quote.delta, gamma=quote.gamma, vega=quote.vega,
                                  theta=quote.theta, rho=quote.rho),
                is_intrinsic=quote.is_intrinsic,
            ))

    ref_threshold = threshold_list[0]
    bucket = market_type.calibration_bucket(instance.resolution_minute)
    historical = np.asarray(store.observations(market.id, bucket))
    mc_gbm = pricing.monte_carlo_gbm_price(
        spot, ref_threshold, minutes_remaining, sigma, "call",
        to_underlying=market_type.to_underlying, simulations=20000)
    mc_bootstrap = (
        pricing.monte_carlo_bootstrap_price(ref_threshold, historical, spot, market_type.payoff_call)
        if len(historical) >= 10 else mc_gbm
    )

    return QuoteResponse(
        market_id=market_id, instance_id=instance_id, minutes_remaining=minutes_remaining,
        spot=spot, sigma=sigma, unit_label=market.unit_label, greek_hints=market_type.greek_hints(),
        contracts=contracts, monte_carlo_gbm_price=mc_gbm, monte_carlo_bootstrap_price=mc_bootstrap,
    )


@app.post("/contracts", response_model=ContractView)
def create_contract(req: CreateContractRequest):
    market, market_type = _market_and_type(req.market_id)
    try:
        instance = platform.instance(req.market_id, req.instance_id)
    except KeyError:
        raise HTTPException(404, "unknown instance")
    if req.contract_type not in ("call", "put"):
        raise HTTPException(400, "contract_type must be 'call' or 'put'")

    minutes_remaining = max(platform.minutes_remaining(instance.resolution_minute), 0.0001)
    spot, sigma = _live_spot(market, market_type, instance)

    contract = settlement.create_contract(
        market, market_type, instance, req.contract_type, req.threshold, spot, sigma, minutes_remaining)
    store.save(contract)
    return _contract_view(contract, market.title)


@app.get("/markets/{market_id}/contracts", response_model=List[ContractView])
def list_contracts(market_id: str):
    market, _ = _market_and_type(market_id)
    return [_contract_view(c, market.title) for c in store.list_for_market(market_id)]


@app.get("/contracts/{contract_id}", response_model=ContractView)
def get_contract(contract_id: str):
    c = store.get(contract_id)
    if c is None:
        raise HTTPException(404, "unknown contract")
    market = store.get_market(c.market_id)
    return _contract_view(c, market.title if market else None)


@app.get("/portfolio", response_model=List[ContractView])
def get_portfolio():
    market_titles = {m.id: m.title for m in store.list_markets()}
    return [_contract_view(c, market_titles.get(c.market_id)) for c in store.list_all_contracts()]


class AdvanceClockRequest(BaseModel):
    minutes: int = 5


@app.post("/clock/advance")
def advance_clock(req: AdvanceClockRequest):
    """
    Advances the simulated clock and auto-settles any OPEN contracts whose
    resolution time has now passed: record_resolution_event (the simulated
    'this instance resolved' event) immediately followed by settle().
    """
    platform.clock_minute += req.minutes
    settled_ids = []
    for c in store.list_all_contracts():
        if c.state != settlement.ContractState.OPEN:
            continue
        if c.resolution_minute > platform.clock_minute:
            continue
        market, market_type = _market_and_type(c.market_id)
        instance = platform.instance(c.market_id, c.instance_id)
        realized = market_type.resolve(market, instance)
        settlement.record_resolution_event(c, realized)
        settlement.settle(c, market_type)
        store.save(c)
        settled_ids.append(c.id)
    return {"clock_minute": platform.clock_minute, "settled_contract_ids": settled_ids}


@app.get("/markets/{market_id}/implied-sigma")
def implied_sigma(market_id: str, instance_id: str, threshold: float,
                   quoted_price: float, contract_type: str = "call"):
    market, market_type = _market_and_type(market_id)
    try:
        instance = platform.instance(market_id, instance_id)
    except KeyError:
        raise HTTPException(404, "unknown instance")
    minutes_remaining = max(platform.minutes_remaining(instance.resolution_minute), 0.0001)
    spot, sigma = _live_spot(market, market_type, instance)
    implied = pricing.implied_sigma(quoted_price, spot, threshold, minutes_remaining,
                                     contract_type, to_underlying=market_type.to_underlying)
    return {"implied_sigma": implied, "historical_sigma": sigma}


class CreateMarketRequest(BaseModel):
    title: str
    market_type: str
    resolution_criterion: str
    unit_label: str
    params: dict = {}


PLACEHOLDER_CRITERIA = {"tbd", "todo", "n/a", ""}


@app.post("/admin/markets", response_model=MarketView)
def create_market(req: CreateMarketRequest):
    """
    Admin-only in intent, not enforced by real access control — there is no
    auth system in this demo, so this is a separate route prefix by
    convention only (stated as a scope limitation in the README).
    """
    if req.market_type not in market_registry.MARKET_TYPES:
        raise HTTPException(400, f"unknown market_type '{req.market_type}'")
    criterion = req.resolution_criterion.strip()
    if len(criterion) < 20 or criterion.lower() in PLACEHOLDER_CRITERIA:
        raise HTTPException(400, "resolution_criterion must be a real, published, checkable criterion")

    market_type = market_registry.get(req.market_type)
    errors = market_type.validate_params(req.params)
    if errors:
        raise HTTPException(400, "; ".join(errors))

    market = Market(
        id=f"{req.market_type}-{uuid.uuid4().hex[:8]}", market_type=req.market_type,
        title=req.title, resolution_criterion=criterion, unit_label=req.unit_label,
        params=req.params,
    )
    store.save_market(market)
    market_type.seed_demo_history(
        market, platform.rng, lambda bucket, value: store.log_observation(market.id, bucket, value))
    platform.generate_instances_for(market, market_type)
    return MarketView(id=market.id, market_type=market.market_type, title=market.title,
                       resolution_criterion=market.resolution_criterion, unit_label=market.unit_label)
