"""
ShuttlePredict API.

Purpose: improve the accuracy and communication of NTU shuttle arrival
predictions by pricing "confidence contracts" on arrival windows, using
derivatives-pricing theory (Black-Scholes, Greeks, Monte Carlo) as the
modeling technique. This is an educational SC2006 project — every price is
a simulated confidence unit, never real currency, and there is no
betting/wagering framing anywhere in this API (see README).
"""
import os
from typing import List, Optional

import numpy as np
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel

from . import db, pricing, settlement, simulator, state

app = FastAPI(title="ShuttlePredict API")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_methods=["*"], allow_headers=["*"],
)

store: db.ContractStore
sim: state.SimulationState


def configure(db_url: str | None = None, seed: int = 42) -> None:
    """
    (Re)builds the module's store/sim instances. Called once at import time
    with defaults, and callable again by tests to get an isolated DB + a
    fresh simulated schedule instead of sharing dev state across test runs.
    """
    global store, sim
    db_url = db_url or os.environ.get("SHUTTLEPREDICT_DB_URL", "sqlite:///shuttlepredict.db")
    store = db.ContractStore(db.make_session_factory(db_url))
    sim = state.SimulationState(seed=seed)
    sim.seed_historical_data(store)


configure()


def _calibrated_sigma(route_id: str, hour_of_day: int) -> float:
    delays = store.historical_delays(route_id, hour_of_day)
    if len(delays) < 10:
        return 0.2  # fallback for a route/hour with no logged history yet
    return simulator.calibrate_sigma(np.asarray(delays))


def _live_predicted_delay(route_id: str, scheduled_minute_of_day: int,
                           trip: simulator.ScheduledTrip, minutes_remaining: float) -> tuple[float, float]:
    """Returns (predicted_delay_minutes, sigma) for a trip right now."""
    hour = (scheduled_minute_of_day // 60) % 24
    sigma = _calibrated_sigma(route_id, hour)
    predicted = simulator.predicted_delay_minutes(
        trip.true_delay_minutes, max(minutes_remaining, 0), minutes_to_horizon=60,
        prediction_sigma=sigma * pricing.SHIFT_MINUTES, rng=sim.rng)
    return predicted, sigma


class TripView(BaseModel):
    scheduled_minute_of_day: int
    minutes_remaining: float
    predicted_delay_minutes: float


class GreeksView(BaseModel):
    delta: float
    gamma: float
    vega: float
    theta: float
    rho: float


class ContractQuoteView(BaseModel):
    contract_type: str
    price: float
    greeks: GreeksView
    is_intrinsic: bool


class QuoteResponse(BaseModel):
    route_id: str
    scheduled_minute_of_day: int
    minutes_remaining: float
    predicted_delay_minutes: float
    sigma: float
    contracts: List[ContractQuoteView]
    monte_carlo_gbm_price: float
    monte_carlo_bootstrap_price: float


class CreateContractRequest(BaseModel):
    route_id: str
    contract_type: str
    threshold_minutes: float
    scheduled_minute_of_day: int


class ContractView(BaseModel):
    id: str
    route_id: str
    contract_type: str
    threshold_minutes: float
    scheduled_minute_of_day: int
    predicted_delay_at_creation: float
    sigma_at_creation: float
    price_at_creation: float
    state: str
    actual_delay_minutes: Optional[float]
    settlement_price: Optional[float]
    in_the_money: Optional[bool]


def _contract_view(c: settlement.Contract) -> ContractView:
    return ContractView(
        id=c.id, route_id=c.route_id, contract_type=c.contract_type,
        threshold_minutes=c.threshold_minutes,
        scheduled_minute_of_day=c.scheduled_minute_of_day,
        predicted_delay_at_creation=c.predicted_delay_at_creation,
        sigma_at_creation=c.sigma_at_creation, price_at_creation=c.price_at_creation,
        state=c.state.value, actual_delay_minutes=c.actual_delay_minutes,
        settlement_price=c.settlement_price, in_the_money=c.in_the_money,
    )


@app.get("/health")
def health():
    return {"status": "ok"}


@app.get("/routes")
def list_routes():
    return state.ROUTES


@app.get("/routes/{route_id}/schedule", response_model=List[TripView])
def get_schedule(route_id: str):
    if route_id not in sim.schedules:
        raise HTTPException(404, "unknown route")
    out = []
    for trip in sim.schedules[route_id]:
        minutes_remaining = sim.minutes_remaining(trip.scheduled_minute_of_day)
        if minutes_remaining < -30:  # drop trips long past, keep the list demo-sized
            continue
        predicted, _ = _live_predicted_delay(route_id, trip.scheduled_minute_of_day, trip, minutes_remaining)
        out.append(TripView(scheduled_minute_of_day=trip.scheduled_minute_of_day,
                             minutes_remaining=minutes_remaining,
                             predicted_delay_minutes=predicted))
    return out


@app.get("/routes/{route_id}/quote", response_model=QuoteResponse)
def get_quote(route_id: str, scheduled_minute_of_day: int, thresholds: str = "0,5,10"):
    if route_id not in sim.schedules:
        raise HTTPException(404, "unknown route")
    try:
        trip = sim.trip(route_id, scheduled_minute_of_day)
    except KeyError:
        raise HTTPException(404, "unknown trip")

    minutes_remaining = max(sim.minutes_remaining(scheduled_minute_of_day), 0.0001)
    predicted_delay, sigma = _live_predicted_delay(route_id, scheduled_minute_of_day, trip, minutes_remaining)

    threshold_list = [float(t) for t in thresholds.split(",")]
    contracts = []
    for threshold in threshold_list:
        for contract_type in ("call", "put"):
            quote = pricing.price_confidence_contract(
                predicted_delay, threshold, minutes_remaining, sigma, contract_type)
            contracts.append(ContractQuoteView(
                contract_type=f"{contract_type}@{threshold}",
                price=quote.price,
                greeks=GreeksView(delta=quote.delta, gamma=quote.gamma,
                                  vega=quote.vega, theta=quote.theta, rho=quote.rho),
                is_intrinsic=quote.is_intrinsic,
            ))

    ref_threshold = threshold_list[0]
    hour = (scheduled_minute_of_day // 60) % 24
    historical = np.asarray(store.historical_delays(route_id, hour))
    mc_gbm = pricing.monte_carlo_gbm_price(
        predicted_delay, ref_threshold, minutes_remaining, sigma, "call", simulations=20000)
    mc_bootstrap = (
        pricing.monte_carlo_bootstrap_price(ref_threshold, historical, predicted_delay, "call")
        if len(historical) >= 10 else mc_gbm
    )

    return QuoteResponse(
        route_id=route_id, scheduled_minute_of_day=scheduled_minute_of_day,
        minutes_remaining=minutes_remaining, predicted_delay_minutes=predicted_delay,
        sigma=sigma, contracts=contracts,
        monte_carlo_gbm_price=mc_gbm, monte_carlo_bootstrap_price=mc_bootstrap,
    )


@app.post("/contracts", response_model=ContractView)
def create_contract(req: CreateContractRequest):
    if req.route_id not in sim.schedules:
        raise HTTPException(404, "unknown route")
    try:
        trip = sim.trip(req.route_id, req.scheduled_minute_of_day)
    except KeyError:
        raise HTTPException(404, "unknown trip")
    if req.contract_type not in ("call", "put"):
        raise HTTPException(400, "contract_type must be 'call' or 'put'")

    minutes_remaining = max(sim.minutes_remaining(req.scheduled_minute_of_day), 0.0001)
    predicted_delay, sigma = _live_predicted_delay(req.route_id, req.scheduled_minute_of_day, trip, minutes_remaining)

    contract = settlement.create_contract(
        req.route_id, req.contract_type, req.threshold_minutes,
        req.scheduled_minute_of_day, predicted_delay, minutes_remaining, sigma)
    store.save(contract)
    return _contract_view(contract)


@app.get("/routes/{route_id}/contracts", response_model=List[ContractView])
def list_contracts(route_id: str):
    return [_contract_view(c) for c in store.list_for_route(route_id)]


@app.get("/contracts/{contract_id}", response_model=ContractView)
def get_contract(contract_id: str):
    c = store.get(contract_id)
    if c is None:
        raise HTTPException(404, "unknown contract")
    return _contract_view(c)


class AdvanceClockRequest(BaseModel):
    minutes: int = 5


@app.post("/clock/advance")
def advance_clock(req: AdvanceClockRequest):
    """
    Advances the simulated clock and auto-settles any CREATED contracts
    whose scheduled arrival has now passed: record_arrival (the simulated
    'bus arrives' event) immediately followed by settle().
    """
    sim.clock_minute += req.minutes
    settled_ids = []
    for route_id in state.ROUTES:
        for c in store.list_for_route(route_id):
            if c.state != settlement.ContractState.CREATED:
                continue
            if c.scheduled_minute_of_day > sim.clock_minute:
                continue
            trip = sim.trip(route_id, c.scheduled_minute_of_day)
            settlement.record_arrival(c, trip.true_delay_minutes)
            settlement.settle(c)
            store.save(c)
            settled_ids.append(c.id)
    return {"clock_minute": sim.clock_minute, "settled_contract_ids": settled_ids}


@app.get("/routes/{route_id}/implied-punctuality")
def implied_punctuality(route_id: str, scheduled_minute_of_day: int, threshold_minutes: float,
                         quoted_price: float, contract_type: str = "call"):
    trip = sim.trip(route_id, scheduled_minute_of_day)
    minutes_remaining = max(sim.minutes_remaining(scheduled_minute_of_day), 0.0001)
    predicted_delay, sigma = _live_predicted_delay(route_id, scheduled_minute_of_day, trip, minutes_remaining)
    implied = pricing.implied_punctuality(
        quoted_price, predicted_delay, threshold_minutes, minutes_remaining, contract_type)
    return {"implied_sigma": implied, "historical_sigma": sigma}
