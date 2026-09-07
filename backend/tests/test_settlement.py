import pytest
from app import settlement


def make_contract(contract_type='call', threshold=5.0, predicted_delay=2.0):
    return settlement.create_contract(
        route_id='R1', contract_type=contract_type, threshold_minutes=threshold,
        scheduled_minute_of_day=600, predicted_delay_minutes=predicted_delay,
        minutes_remaining=15, sigma=0.2)


def test_full_lifecycle_call_in_the_money():
    c = make_contract(contract_type='call', threshold=5.0)
    assert c.state == settlement.ContractState.CREATED
    assert c.price_at_creation > 0

    settlement.record_arrival(c, actual_delay_minutes=1.0)  # arrives well before threshold
    assert c.state == settlement.ContractState.ARRIVED

    settlement.settle(c)
    assert c.state == settlement.ContractState.SETTLED
    assert c.in_the_money is True
    assert c.settlement_price == pytest.approx(4.0)  # payoff_call(1.0, 5.0)


def test_full_lifecycle_call_out_of_the_money():
    c = make_contract(contract_type='call', threshold=5.0)
    settlement.record_arrival(c, actual_delay_minutes=12.0)  # arrives well after threshold
    settlement.settle(c)
    assert c.in_the_money is False
    assert c.settlement_price == 0.0


def test_cannot_settle_before_arrival():
    c = make_contract()
    with pytest.raises(settlement.InvalidTransition):
        settlement.settle(c)


def test_cannot_record_arrival_twice():
    c = make_contract()
    settlement.record_arrival(c, actual_delay_minutes=3.0)
    with pytest.raises(settlement.InvalidTransition):
        settlement.record_arrival(c, actual_delay_minutes=4.0)


def test_expire_from_created():
    c = make_contract()
    settlement.expire(c)
    assert c.state == settlement.ContractState.EXPIRED


def test_cannot_expire_after_settled():
    c = make_contract()
    settlement.record_arrival(c, actual_delay_minutes=1.0)
    settlement.settle(c)
    with pytest.raises(settlement.InvalidTransition):
        settlement.expire(c)
