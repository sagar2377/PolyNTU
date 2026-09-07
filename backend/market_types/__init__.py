"""
Importing this package registers all built-in market types with
core.market's MARKET_TYPES registry as a side effect. app/main.py imports
this package once at startup; nothing else needs to import the plugin
modules directly.
"""
from core import market

from .shuttle_arrival.market_type import ShuttleArrivalMarketType
from .student_election.market_type import StudentElectionMarketType

market.register(ShuttleArrivalMarketType())
market.register(StudentElectionMarketType())
