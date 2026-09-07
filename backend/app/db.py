"""SQLite persistence for contracts and the simulated historical delay log."""
from contextlib import contextmanager
from datetime import datetime, timezone

from sqlalchemy import create_engine, Column, String, Float, Integer, DateTime, Boolean
from sqlalchemy.orm import declarative_base, sessionmaker

from .settlement import Contract, ContractState

Base = declarative_base()


class ContractRow(Base):
    __tablename__ = "contracts"

    id = Column(String, primary_key=True)
    route_id = Column(String, nullable=False)
    contract_type = Column(String, nullable=False)
    threshold_minutes = Column(Float, nullable=False)
    scheduled_minute_of_day = Column(Integer, nullable=False)
    predicted_delay_at_creation = Column(Float, nullable=False)
    sigma_at_creation = Column(Float, nullable=False)
    price_at_creation = Column(Float, nullable=False)
    state = Column(String, nullable=False)
    created_at = Column(DateTime, nullable=False)
    actual_delay_minutes = Column(Float, nullable=True)
    arrived_at = Column(DateTime, nullable=True)
    settlement_price = Column(Float, nullable=True)
    in_the_money = Column(Boolean, nullable=True)
    settled_at = Column(DateTime, nullable=True)


class HistoricalDelayRow(Base):
    __tablename__ = "historical_delays"

    id = Column(Integer, primary_key=True, autoincrement=True)
    route_id = Column(String, nullable=False)
    hour_of_day = Column(Integer, nullable=False)
    delay_minutes = Column(Float, nullable=False)
    recorded_at = Column(DateTime, nullable=False)


def to_row(c: Contract) -> ContractRow:
    return ContractRow(
        id=c.id, route_id=c.route_id, contract_type=c.contract_type,
        threshold_minutes=c.threshold_minutes,
        scheduled_minute_of_day=c.scheduled_minute_of_day,
        predicted_delay_at_creation=c.predicted_delay_at_creation,
        sigma_at_creation=c.sigma_at_creation, price_at_creation=c.price_at_creation,
        state=c.state.value, created_at=c.created_at,
        actual_delay_minutes=c.actual_delay_minutes, arrived_at=c.arrived_at,
        settlement_price=c.settlement_price, in_the_money=c.in_the_money,
        settled_at=c.settled_at,
    )


def from_row(row: ContractRow) -> Contract:
    return Contract(
        id=row.id, route_id=row.route_id, contract_type=row.contract_type,
        threshold_minutes=row.threshold_minutes,
        scheduled_minute_of_day=row.scheduled_minute_of_day,
        predicted_delay_at_creation=row.predicted_delay_at_creation,
        sigma_at_creation=row.sigma_at_creation, price_at_creation=row.price_at_creation,
        state=ContractState(row.state), created_at=row.created_at,
        actual_delay_minutes=row.actual_delay_minutes, arrived_at=row.arrived_at,
        settlement_price=row.settlement_price, in_the_money=row.in_the_money,
        settled_at=row.settled_at,
    )


def make_session_factory(db_url: str = "sqlite:///shuttlepredict.db"):
    engine = create_engine(db_url, connect_args={"check_same_thread": False} if "sqlite" in db_url else {})
    Base.metadata.create_all(engine)
    return sessionmaker(bind=engine)


class ContractStore:
    """Thin repository wrapping SQLAlchemy so main.py doesn't touch sessions directly."""

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

    def save(self, contract: Contract) -> None:
        with self._session() as session:
            session.merge(to_row(contract))

    def get(self, contract_id: str) -> Contract | None:
        with self._session() as session:
            row = session.get(ContractRow, contract_id)
            return from_row(row) if row else None

    def list_for_route(self, route_id: str) -> list[Contract]:
        with self._session() as session:
            rows = session.query(ContractRow).filter_by(route_id=route_id).all()
            return [from_row(r) for r in rows]

    def log_historical_delay(self, route_id: str, hour_of_day: int, delay_minutes: float) -> None:
        with self._session() as session:
            session.add(HistoricalDelayRow(
                route_id=route_id, hour_of_day=hour_of_day,
                delay_minutes=delay_minutes, recorded_at=datetime.now(timezone.utc)))

    def historical_delays(self, route_id: str, hour_of_day: int) -> list[float]:
        with self._session() as session:
            rows = session.query(HistoricalDelayRow).filter_by(
                route_id=route_id, hour_of_day=hour_of_day).all()
            return [r.delay_minutes for r in rows]
