from core import db
from core.market import Market, MarketInstance, MarketType
from core import settlement


class ToyMarketType(MarketType):
    key = "toy"
    delta_sign = -1.0

    def generate_instances(self, market, rng):
        return []

    def live_spot(self, market, instance, now_minute, rng, observations):
        return 0.0, 0.2

    def resolve(self, market, instance):
        return 0.0

    def to_underlying(self, value):
        return -value + 50.0

    def payoff_call(self, realized, threshold):
        return max(threshold - realized, 0.0)

    def payoff_put(self, realized, threshold):
        return max(realized - threshold, 0.0)


MARKET_TYPE = ToyMarketType()


def make_store():
    return db.Store(db.make_session_factory("sqlite:///:memory:"))


def make_market(market_id="m1"):
    return Market(id=market_id, market_type="toy", title="Toy Market",
                  resolution_criterion="Settles against a fixed truth_seed value.",
                  unit_label="units", params={"k": "v"})


def make_contract(market_id="m1", contract_type='call', threshold=5.0, spot=2.0):
    market = make_market(market_id)
    instance = MarketInstance(id=f"{market_id}:600", market_id=market_id, resolution_minute=600,
                               truth_seed={})
    return settlement.create_contract(
        market, MARKET_TYPE, instance, contract_type, threshold, spot, sigma=0.2, minutes_remaining=15)


def test_save_and_get_market_round_trip():
    store = make_store()
    market = make_market()
    store.save_market(market)

    fetched = store.get_market(market.id)
    assert fetched is not None
    assert fetched.title == market.title
    assert fetched.resolution_criterion == market.resolution_criterion
    assert fetched.params == {"k": "v"}
    assert fetched.is_active is True


def test_list_markets_filters_inactive_by_default():
    store = make_store()
    active = make_market("m1")
    inactive = make_market("m2")
    inactive.is_active = False
    store.save_market(active)
    store.save_market(inactive)

    assert {m.id for m in store.list_markets()} == {"m1"}
    assert {m.id for m in store.list_markets(active_only=False)} == {"m1", "m2"}


def test_save_and_get_contract_round_trip():
    store = make_store()
    c = make_contract()
    store.save(c)

    fetched = store.get(c.id)
    assert fetched is not None
    assert fetched.id == c.id
    assert fetched.state == settlement.ContractState.OPEN
    assert fetched.price_at_creation == c.price_at_creation


def test_save_reflects_state_transitions():
    store = make_store()
    c = make_contract(contract_type='put', threshold=5.0)
    store.save(c)

    settlement.record_resolution_event(c, realized_value=9.0)
    settlement.settle(c, MARKET_TYPE)
    store.save(c)

    fetched = store.get(c.id)
    assert fetched.state == settlement.ContractState.SETTLED
    assert fetched.in_the_money is True
    assert fetched.settlement_price == 4.0


def test_list_for_market_and_list_all_contracts():
    store = make_store()
    a = make_contract("m1")
    b = make_contract("m2")
    store.save(a)
    store.save(b)

    assert {c.id for c in store.list_for_market("m1")} == {a.id}
    assert {c.id for c in store.list_all_contracts()} == {a.id, b.id}


def test_observation_log_round_trip():
    store = make_store()
    store.log_observation("m1", "bucketA", 3.5)
    store.log_observation("m1", "bucketA", 4.5)
    store.log_observation("m1", "bucketB", 1.0)

    assert sorted(store.observations("m1", "bucketA")) == [3.5, 4.5]
    assert store.observations("m1", "bucketB") == [1.0]
