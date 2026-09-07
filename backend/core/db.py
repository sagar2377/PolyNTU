"""SQLite persistence for markets, contracts, and the historical observation log."""
import json
from contextlib import contextmanager
from datetime import datetime, timezone

from sqlalchemy import create_engine, Column, String, Float, Integer, DateTime, Boolean, Text
from sqlalchemy.orm import declarative_base, sessionmaker

from .market import Market
from .settlement import Contract, ContractState

Base = declarative_base()


class MarketRow(Base):
    __tablename__ = "markets"

    id = Column(String, primary_key=True)
    market_type = Column(String, nullable=False)
    title = Column(String, nullable=False)
    resolution_criterion = Column(Text, nullable=False)
    unit_label = Column(String, nullable=False)
    params_json = Column(Text, nullable=False)
    is_active = Column(Boolean, nullable=False, default=True)
    created_at = Column(DateTime, nullable=False)


class ContractRow(Base):
    __tablename__ = "contracts"

    id = Column(String, primary_key=True)
    market_id = Column(String, nullable=False)
    instance_id = Column(String, nullable=False)
    contract_type = Column(String, nullable=False)
    threshold = Column(Float, nullable=False)
    resolution_minute = Column(Integer, nullable=False)
    spot_at_creation = Column(Float, nullable=False)
    sigma_at_creation = Column(Float, nullable=False)
    price_at_creation = Column(Float, nullable=False)
    state = Column(String, nullable=False)
    created_at = Column(DateTime, nullable=False)
    realized_value = Column(Float, nullable=True)
    resolved_at = Column(DateTime, nullable=True)
    settlement_price = Column(Float, nullable=True)
    in_the_money = Column(Boolean, nullable=True)
    settled_at = Column(DateTime, nullable=True)


class ObservationRow(Base):
    __tablename__ = "observations"

    id = Column(Integer, primary_key=True, autoincrement=True)
    market_id = Column(String, nullable=False)
    calibration_bucket = Column(String, nullable=False)
    value = Column(Float, nullable=False)
    recorded_at = Column(DateTime, nullable=False)


def _market_to_row(m: Market) -> MarketRow:
    return MarketRow(
        id=m.id, market_type=m.market_type, title=m.title,
        resolution_criterion=m.resolution_criterion, unit_label=m.unit_label,
        params_json=json.dumps(m.params), is_active=m.is_active,
        created_at=datetime.now(timezone.utc),
    )


def _market_from_row(row: MarketRow) -> Market:
    return Market(
        id=row.id, market_type=row.market_type, title=row.title,
        resolution_criterion=row.resolution_criterion, unit_label=row.unit_label,
        params=json.loads(row.params_json), is_active=row.is_active,
    )


def _contract_to_row(c: Contract) -> ContractRow:
    return ContractRow(
        id=c.id, market_id=c.market_id, instance_id=c.instance_id,
        contract_type=c.contract_type, threshold=c.threshold,
        resolution_minute=c.resolution_minute, spot_at_creation=c.spot_at_creation,
        sigma_at_creation=c.sigma_at_creation, price_at_creation=c.price_at_creation,
        state=c.state.value, created_at=c.created_at,
        realized_value=c.realized_value, resolved_at=c.resolved_at,
        settlement_price=c.settlement_price, in_the_money=c.in_the_money,
        settled_at=c.settled_at,
    )


def _contract_from_row(row: ContractRow) -> Contract:
    return Contract(
        id=row.id, market_id=row.market_id, instance_id=row.instance_id,
        contract_type=row.contract_type, threshold=row.threshold,
        resolution_minute=row.resolution_minute, spot_at_creation=row.spot_at_creation,
        sigma_at_creation=row.sigma_at_creation, price_at_creation=row.price_at_creation,
        state=ContractState(row.state), created_at=row.created_at,
        realized_value=row.realized_value, resolved_at=row.resolved_at,
        settlement_price=row.settlement_price, in_the_money=row.in_the_money,
        settled_at=row.settled_at,
    )


def make_session_factory(db_url: str = "sqlite:///polyntu.db"):
    engine = create_engine(db_url, connect_args={"check_same_thread": False} if "sqlite" in db_url else {})
    Base.metadata.create_all(engine)
    return sessionmaker(bind=engine)


class Store:
    """
    Thin repository wrapping SQLAlchemy so app/main.py doesn't touch
    sessions directly. Kept as one class (markets + contracts +
    observations) rather than three separate repositories — that split
    would be premature separation at this project's scale.
    """

    def __init__(self, session_factory):
        self._session_factory = session_factory

    @contextmanager
    def _session(self):
        session = self._session_factory()
        try:
            yield session
            session.commit()
        finally:
            session.close()

    # -- markets --------------------------------------------------------
    def save_market(self, market: Market) -> None:
        with self._session() as session:
            session.merge(_market_to_row(market))

    def get_market(self, market_id: str) -> Market | None:
        with self._session() as session:
            row = session.get(MarketRow, market_id)
            return _market_from_row(row) if row else None

    def list_markets(self, active_only: bool = True) -> list[Market]:
        with self._session() as session:
            query = session.query(MarketRow)
            if active_only:
                query = query.filter_by(is_active=True)
            return [_market_from_row(r) for r in query.all()]

    # -- contracts --------------------------------------------------------
    def save(self, contract: Contract) -> None:
        with self._session() as session:
            session.merge(_contract_to_row(contract))

    def get(self, contract_id: str) -> Contract | None:
        with self._session() as session:
            row = session.get(ContractRow, contract_id)
            return _contract_from_row(row) if row else None

    def list_for_market(self, market_id: str) -> list[Contract]:
        with self._session() as session:
            rows = session.query(ContractRow).filter_by(market_id=market_id).all()
            return [_contract_from_row(r) for r in rows]

    def list_all_contracts(self) -> list[Contract]:
        with self._session() as session:
            rows = session.query(ContractRow).all()
            return [_contract_from_row(r) for r in rows]

    # -- observations (historical values, used for volatility calibration) --
    def log_observation(self, market_id: str, calibration_bucket: str, value: float) -> None:
        with self._session() as session:
            session.add(ObservationRow(
                market_id=market_id, calibration_bucket=calibration_bucket,
                value=value, recorded_at=datetime.now(timezone.utc)))

    def observations(self, market_id: str, calibration_bucket: str) -> list[float]:
        with self._session() as session:
            rows = session.query(ObservationRow).filter_by(
                market_id=market_id, calibration_bucket=calibration_bucket).all()
            return [r.value for r in rows]
