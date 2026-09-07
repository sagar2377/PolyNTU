import pytest
from fastapi.testclient import TestClient

from app import main


@pytest.fixture()
def client(tmp_path):
    main.configure(db_url=f"sqlite:///{tmp_path}/test.db", seed=1)
    return TestClient(main.app)


def test_health(client):
    assert client.get("/health").json() == {"status": "ok"}


def test_list_routes(client):
    assert client.get("/routes").json() == main.state.ROUTES


def test_schedule_returns_upcoming_trips(client):
    route_id = main.state.ROUTES[0]
    resp = client.get(f"/routes/{route_id}/schedule")
    assert resp.status_code == 200
    trips = resp.json()
    assert len(trips) > 0
    assert all("predicted_delay_minutes" in t for t in trips)


def test_quote_returns_call_and_put_for_each_threshold(client):
    route_id = main.state.ROUTES[0]
    trip = main.sim.schedules[route_id][0]
    resp = client.get(f"/routes/{route_id}/quote",
                       params={"scheduled_minute_of_day": trip.scheduled_minute_of_day,
                               "thresholds": "0,5,10"})
    assert resp.status_code == 200
    body = resp.json()
    labels = {c["contract_type"] for c in body["contracts"]}
    assert labels == {"call@0.0", "put@0.0", "call@5.0", "put@5.0", "call@10.0", "put@10.0"}
    for c in body["contracts"]:
        assert c["price"] >= 0
        assert "delta" in c["greeks"]


def test_create_contract_and_full_settlement_lifecycle(client):
    route_id = main.state.ROUTES[0]
    trip = main.sim.schedules[route_id][0]

    create_resp = client.post("/contracts", json={
        "route_id": route_id, "contract_type": "call",
        "threshold_minutes": 5.0, "scheduled_minute_of_day": trip.scheduled_minute_of_day,
    })
    assert create_resp.status_code == 200
    contract = create_resp.json()
    assert contract["state"] == "created"

    # advance the clock well past this trip's scheduled time to trigger auto-settlement
    minutes_to_advance = (trip.scheduled_minute_of_day - main.sim.clock_minute) + 1
    advance_resp = client.post("/clock/advance", json={"minutes": minutes_to_advance})
    assert advance_resp.status_code == 200
    assert contract["id"] in advance_resp.json()["settled_contract_ids"]

    settled = client.get(f"/contracts/{contract['id']}").json()
    assert settled["state"] == "settled"
    assert settled["in_the_money"] is not None


def test_unknown_route_returns_404(client):
    assert client.get("/routes/does-not-exist/schedule").status_code == 404


def test_invalid_contract_type_returns_400(client):
    route_id = main.state.ROUTES[0]
    trip = main.sim.schedules[route_id][0]
    resp = client.post("/contracts", json={
        "route_id": route_id, "contract_type": "bet",
        "threshold_minutes": 5.0, "scheduled_minute_of_day": trip.scheduled_minute_of_day,
    })
    assert resp.status_code == 400
