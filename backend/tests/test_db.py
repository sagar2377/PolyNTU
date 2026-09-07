from app import db, settlement


def make_store():
    return db.ContractStore(db.make_session_factory("sqlite:///:memory:"))


def test_save_and_get_round_trip():
    store = make_store()
    c = settlement.create_contract(
        route_id='R1', contract_type='call', threshold_minutes=5.0,
        scheduled_minute_of_day=600, predicted_delay_minutes=2.0,
        minutes_remaining=15, sigma=0.2)
    store.save(c)

    fetched = store.get(c.id)
    assert fetched is not None
    assert fetched.id == c.id
    assert fetched.state == settlement.ContractState.CREATED
    assert fetched.price_at_creation == c.price_at_creation


def test_save_reflects_state_transitions():
    store = make_store()
    c = settlement.create_contract(
        route_id='R1', contract_type='put', threshold_minutes=5.0,
        scheduled_minute_of_day=600, predicted_delay_minutes=2.0,
        minutes_remaining=15, sigma=0.2)
    store.save(c)

    settlement.record_arrival(c, actual_delay_minutes=9.0)
    settlement.settle(c)
    store.save(c)

    fetched = store.get(c.id)
    assert fetched.state == settlement.ContractState.SETTLED
    assert fetched.in_the_money is True
    assert fetched.settlement_price == 4.0


def test_list_for_route_returns_only_that_route():
    store = make_store()
    a = settlement.create_contract('R1', 'call', 5.0, 600, 2.0, 15, 0.2)
    b = settlement.create_contract('R2', 'call', 5.0, 600, 2.0, 15, 0.2)
    store.save(a)
    store.save(b)

    r1_contracts = store.list_for_route('R1')
    assert {c.id for c in r1_contracts} == {a.id}


def test_historical_delay_log_round_trip():
    store = make_store()
    store.log_historical_delay('R1', hour_of_day=8, delay_minutes=3.5)
    store.log_historical_delay('R1', hour_of_day=8, delay_minutes=4.5)
    store.log_historical_delay('R1', hour_of_day=14, delay_minutes=1.0)

    delays = store.historical_delays('R1', hour_of_day=8)
    assert sorted(delays) == [3.5, 4.5]
