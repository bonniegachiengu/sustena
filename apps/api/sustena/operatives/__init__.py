"""
sustena/operatives/__init__.py

Public API for the operatives package.
"""

import pathlib

# Directory containing JSON graph spec files for operatives (Sprint 5.5+)
GRAPHS_DIR: pathlib.Path = pathlib.Path(__file__).parent / "graphs"

from sustena.operatives.base import (
    BaseOperative,
    OperativeProposal,
    OperativeVote,
    VoteChoice,
)
from sustena.operatives.mentor    import MentorOperative
from sustena.operatives.protege   import ProtegeOperative
from sustena.operatives.attache   import AttacheOperative
from sustena.operatives.navigator import NavigatorOperative
from sustena.operatives.curator   import CuratorOperative

__all__ = [
    "BaseOperative",
    "MentorOperative",
    "ProtegeOperative",
    "AttacheOperative",
    "NavigatorOperative",
    "CuratorOperative",
    "OperativeVote",
    "OperativeProposal",
    "VoteChoice",
]
