import pytest
from core import settlement
from core.market import Market, MarketInstance, MarketType


class ToyMarketType(MarketType):
    """Minimal concrete MarketType for exercising settlement in isolation."""
    key = "toy"
    delta_sign = -1.0

    def generate_instances(self, market, rng):
        return []

    def live_spot(self, market, instance, now_minute, rng, observations):
        return 0.0, 0.2

    def resolve(self, market, instance):
        return instance.truth_seed["true_value"]

    def to_underlying(self, value):
        return -value + 50.0

    def payoff_call(self, realized, threshold):
        return max(threshold - realized, 0.0)

    def payoff_put(self, realized, threshold):
        return max(realized - threshold, 0.0)


MARKET = Market(id="m1", market_type="toy", title="Toy Market",
                resolution_criterion="Settles against a fixed truth_seed value.",
                unit_label="units", params={})
MARKET_TYPE = ToyMarketType()


def make_contract(contract_type='call', threshold=5.0, spot=2.0):
    instance = MarketInstance(id="m1:600", market_id="m1", resolution_minute=600,
                               truth_seed={"true_value": None})
    return settlement.create_contract(
        MARKET, MARKET_TYPE, instance, contract_type, threshold, spot, sigma=0.2, minutes_remaining=15)


def test_full_lifecycle_call_in_the_money():
    c = make_contract(contract_type='call', threshold=5.0)
    assert c.state == settlement.ContractState.OPEN
    assert c.price_at_creation > 0

    settlement.record_resolution_event(c, realized_value=1.0)  # comfortably below threshold
    assert c.state == settlement.ContractState.RESOLVING

    settlement.settle(c, MARKET_TYPE)
    assert c.state == settlement.ContractState.SETTLED
    assert c.in_the_money is True
    assert c.settlement_price == pytest.approx(4.0)  # payoff_call(1.0, 5.0)


def test_full_lifecycle_call_out_of_the_money():
    c = make_contract(contract_type='call', threshold=5.0)
    settlement.record_resolution_event(c, realized_value=12.0)  # comfortably above threshold
    settlement.settle(c, MARKET_TYPE)
    assert c.in_the_money is False
    assert c.settlement_price == 0.0


def test_cannot_settle_before_resolution_event():
    c = make_contract()
    with pytest.raises(settlement.InvalidTransition):
        settlement.settle(c, MARKET_TYPE)


def test_cannot_record_resolution_event_twice():
    c = make_contract()
    settlement.record_resolution_event(c, realized_value=3.0)
    with pytest.raises(settlement.InvalidTransition):
        settlement.record_resolution_event(c, realized_value=4.0)


def test_expire_from_open():
    c = make_contract()
    settlement.expire(c)
    assert c.state == settlement.ContractState.EXPIRED


def test_cannot_expire_after_settled():
    c = make_contract()
    settlement.record_resolution_event(c, realized_value=1.0)
    settlement.settle(c, MARKET_TYPE)
    with pytest.raises(settlement.InvalidTransition):
        settlement.expire(c)
