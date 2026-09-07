import pytest
from fastapi.testclient import TestClient

from app import main


@pytest.fixture()
def client(tmp_path):
    main.configure(db_url=f"sqlite:///{tmp_path}/test.db", seed=1)
    return TestClient(main.app)


def test_health(client):
    assert client.get("/health").json() == {"status": "ok"}


def test_list_markets_includes_all_default_markets(client):
    markets = client.get("/markets").json()
    ids = {m["id"] for m in markets}
    assert ids == {"shuttle-ntu-blue", "shuttle-ntu-red", "demo-hall-election-candidate-a"}
    for m in markets:
        assert m["resolution_criterion"]  # every market publishes a resolution criterion


def test_shuttle_market_instances_and_quote(client):
    instances = client.get("/markets/shuttle-ntu-blue/instances").json()
    assert len(instances) > 0
    instance_id = instances[0]["instance_id"]

    resp = client.get("/markets/shuttle-ntu-blue/quote",
                       params={"instance_id": instance_id, "thresholds": "0,5,10"})
    assert resp.status_code == 200
    body = resp.json()
    labels = {(c["contract_type"], c["threshold"]) for c in body["contracts"]}
    assert labels == {("call", 0.0), ("put", 0.0), ("call", 5.0), ("put", 5.0), ("call", 10.0), ("put", 10.0)}
    for c in body["contracts"]:
        assert c["price"] >= 0
        assert "delta" in c["greeks"]
        assert c["label"]


def test_election_market_instances_and_quote(client):
    instances = client.get("/markets/demo-hall-election-candidate-a/instances").json()
    assert len(instances) == 1
    instance_id = instances[0]["instance_id"]

    resp = client.get("/markets/demo-hall-election-candidate-a/quote",
                       params={"instance_id": instance_id, "thresholds": "40,50,60"})
    assert resp.status_code == 200
    body = resp.json()
    assert body["unit_label"] == "% vote share"
    assert len(body["contracts"]) == 6


def test_create_contract_and_full_settlement_lifecycle_shuttle(client):
    instances = client.get("/markets/shuttle-ntu-blue/instances").json()
    instance = instances[0]

    create_resp = client.post("/contracts", json={
        "market_id": "shuttle-ntu-blue", "instance_id": instance["instance_id"],
        "contract_type": "call", "threshold": 5.0,
    })
    assert create_resp.status_code == 200
    contract = create_resp.json()
    assert contract["state"] == "open"
    assert contract["market_title"] == "NTU Blue Line Shuttle Arrival"

    minutes_to_advance = int(instance["minutes_remaining"]) + 1
    advance_resp = client.post("/clock/advance", json={"minutes": minutes_to_advance})
    assert advance_resp.status_code == 200
    assert contract["id"] in advance_resp.json()["settled_contract_ids"]

    settled = client.get(f"/contracts/{contract['id']}").json()
    assert settled["state"] == "settled"
    assert settled["in_the_money"] is not None


def test_create_contract_and_settlement_election(client):
    instances = client.get("/markets/demo-hall-election-candidate-a/instances").json()
    instance = instances[0]

    create_resp = client.post("/contracts", json={
        "market_id": "demo-hall-election-candidate-a", "instance_id": instance["instance_id"],
        "contract_type": "call", "threshold": 50.0,
    })
    assert create_resp.status_code == 200
    contract_id = create_resp.json()["id"]

    minutes_to_advance = int(instance["minutes_remaining"]) + 1
    client.post("/clock/advance", json={"minutes": minutes_to_advance})

    settled = client.get(f"/contracts/{contract_id}").json()
    assert settled["state"] == "settled"


def test_portfolio_spans_multiple_markets(client):
    blue_instance = client.get("/markets/shuttle-ntu-blue/instances").json()[0]
    election_instance = client.get("/markets/demo-hall-election-candidate-a/instances").json()[0]

    client.post("/contracts", json={"market_id": "shuttle-ntu-blue",
                                     "instance_id": blue_instance["instance_id"],
                                     "contract_type": "call", "threshold": 5.0})
    client.post("/contracts", json={"market_id": "demo-hall-election-candidate-a",
                                     "instance_id": election_instance["instance_id"],
                                     "contract_type": "put", "threshold": 50.0})

    portfolio = client.get("/portfolio").json()
    assert {c["market_id"] for c in portfolio} == {"shuttle-ntu-blue", "demo-hall-election-candidate-a"}


def test_unknown_market_returns_404(client):
    assert client.get("/markets/does-not-exist/instances").status_code == 404


def test_invalid_contract_type_returns_400(client):
    instance = client.get("/markets/shuttle-ntu-blue/instances").json()[0]
    resp = client.post("/contracts", json={
        "market_id": "shuttle-ntu-blue", "instance_id": instance["instance_id"],
        "contract_type": "bet", "threshold": 5.0,
    })
    assert resp.status_code == 400


def test_admin_create_market_rejects_real_election(client):
    resp = client.post("/admin/markets", json={
        "title": "Real Election", "market_type": "student_election",
        "resolution_criterion": "Settles against the official real vote count.",
        "unit_label": "% vote share",
        "params": {"candidate": "Someone", "is_fictional": False},
    })
    assert resp.status_code == 400
    assert "is_fictional" in resp.json()["detail"]


def test_admin_create_market_rejects_placeholder_resolution_criterion(client):
    resp = client.post("/admin/markets", json={
        "title": "New Market", "market_type": "shuttle_arrival",
        "resolution_criterion": "TBD",
        "unit_label": "minutes of delay",
        "params": {"route_id": "NTU-green"},
    })
    assert resp.status_code == 400


def test_admin_create_market_succeeds_with_valid_fictional_election(client):
    resp = client.post("/admin/markets", json={
        "title": "Second Demo Election", "market_type": "student_election",
        "resolution_criterion": "Settles against the simulated final count for this fictional demo election.",
        "unit_label": "% vote share",
        "params": {"candidate": "Candidate B", "is_fictional": True, "mean_pct": 45.0, "spread_pct": 8.0},
    })
    assert resp.status_code == 200
    new_market_id = resp.json()["id"]
    assert len(client.get(f"/markets/{new_market_id}/instances").json()) == 1
