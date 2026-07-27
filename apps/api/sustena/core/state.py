"""
sustena/core/state.py

StateAccessor — the State primitive.

Wraps a JSON dict (the sustain's live state) and provides safe, typed
dot-path read/write operations. Every operator receives a StateAccessor
instead of the raw dict — this prevents accidental direct mutation and
enforces path validation.

Dot-path examples:
  "finances.liquid.balance"       → state["finances"]["liquid"]["balance"]
  "staff.roster[0].name"          → state["staff"]["roster"][0]["name"]

All mutations are tracked in self._mutations so the operator runner can
diff pre/post state for the event log.
"""

import copy
import re
import uuid
from typing import Any


class StatePathError(Exception):
    """Raised when a dot-path doesn't resolve to a valid location in state."""


class StateValueError(Exception):
    """Raised when a value operation violates a constraint (e.g. negative balance)."""


class StateAccessor:
    """
    Safe, typed interface to a sustain's state dict.

    Usage:
        state = StateAccessor({"finances": {"liquid": {"balance": 50000}}})
        balance = state.get("finances.liquid.balance")   # → 50000
        state.set("finances.liquid.balance", 45000)
        state.increment("finances.liquid.balance", 5000)  # → 50000
    """

    # Regex to split path segments, handling array notation like roster[0]
    _SEGMENT_RE = re.compile(r"(\w+)(?:\[(\d+)\])?")

    def __init__(self, data: dict) -> None:
        # Work on a deep copy — operators never mutate the original until committed
        self._data: dict = copy.deepcopy(data)
        self._original: dict = copy.deepcopy(data)
        self._mutations: list[dict] = []

    # ── Internal path resolution ───────────────────────────────────────────────

    def _resolve(self, path: str) -> tuple[Any, str | int]:
        """
        Walk the path to the parent container and return (parent, final_key).
        Raises StatePathError if any intermediate segment is missing.
        """
        segments = path.split(".")
        node: Any = self._data

        for segment in segments[:-1]:
            m = self._SEGMENT_RE.fullmatch(segment)
            if not m:
                raise StatePathError(f"Invalid path segment: '{segment}' in '{path}'")
            key, idx = m.group(1), m.group(2)

            if not isinstance(node, dict) or key not in node:
                raise StatePathError(f"Path not found: '{key}' in '{path}'")
            node = node[key]

            if idx is not None:
                index = int(idx)
                if not isinstance(node, list) or index >= len(node):
                    raise StatePathError(f"Index [{idx}] out of range in '{path}'")
                node = node[index]

        # Final segment
        last = segments[-1]
        m = self._SEGMENT_RE.fullmatch(last)
        if not m:
            raise StatePathError(f"Invalid final segment: '{last}' in '{path}'")
        key, idx = m.group(1), m.group(2)

        if idx is not None:
            if not isinstance(node, dict) or key not in node:
                raise StatePathError(f"Path not found: '{key}' in '{path}'")
            node = node[key]
            index = int(idx)
            return node, index

        return node, key

    # ── Public API ─────────────────────────────────────────────────────────────

    def get(self, path: str, default: Any = None) -> Any:
        """Read a value at path. Returns default if path doesn't exist."""
        try:
            parent, key = self._resolve(path)
            if isinstance(parent, (dict, list)):
                return parent[key] if isinstance(parent, list) else parent.get(key, default)
            return default
        except (StatePathError, KeyError, IndexError):
            return default

    def get_strict(self, path: str) -> Any:
        """Read a value at path. Raises StatePathError if path doesn't exist."""
        parent, key = self._resolve(path)
        if isinstance(parent, dict):
            if key not in parent:
                raise StatePathError(f"Key '{key}' not found at path '{path}'")
            return parent[key]
        if isinstance(parent, list):
            return parent[key]
        raise StatePathError(f"Cannot read from '{type(parent).__name__}' at '{path}'")

    def set(self, path: str, value: Any) -> None:
        """Set a value at path. Creates intermediate dicts if they don't exist."""
        segments = path.split(".")
        node = self._data

        for segment in segments[:-1]:
            m = self._SEGMENT_RE.fullmatch(segment)
            if not m:
                raise StatePathError(f"Invalid segment '{segment}' in '{path}'")
            key, idx = m.group(1), m.group(2)
            if key not in node:
                node[key] = {}
            node = node[key]
            if idx is not None:
                node = node[int(idx)]

        last = segments[-1]
        m = self._SEGMENT_RE.fullmatch(last)
        if not m:
            raise StatePathError(f"Invalid final segment '{last}' in '{path}'")
        key, idx = m.group(1), m.group(2)

        if idx is not None:
            old_value = node[key][int(idx)] if isinstance(node.get(key), list) and int(idx) < len(node[key]) else None
            node[key][int(idx)] = value
            self._mutations.append({"op": "set", "path": path, "old": old_value, "new": value})
        else:
            old_value = node.get(key)
            node[key] = value
            self._mutations.append({"op": "set", "path": path, "old": old_value, "new": value})

    def exists(self, path: str) -> bool:
        """Return True if path resolves to an existing value (even if None)."""
        try:
            parent, key = self._resolve(path)
            if isinstance(parent, dict):
                return key in parent
            if isinstance(parent, list):
                return 0 <= key < len(parent)
            return False
        except StatePathError:
            return False

    def increment(self, path: str, delta: float) -> float:
        """Add delta to a numeric value. Returns new value."""
        current = self.get_strict(path)
        if not isinstance(current, (int, float)):
            raise StateValueError(f"Cannot increment non-numeric value at '{path}': {current!r}")
        new_val = current + delta
        self.set(path, new_val)
        return new_val

    def decrement(self, path: str, delta: float, allow_negative: bool = False) -> float:
        """Subtract delta from a numeric value. Raises if result < 0 (unless allow_negative)."""
        current = self.get_strict(path)
        if not isinstance(current, (int, float)):
            raise StateValueError(f"Cannot decrement non-numeric value at '{path}': {current!r}")
        new_val = current - delta
        if not allow_negative and new_val < 0:
            raise StateValueError(
                f"Decrement would make '{path}' negative: {current} - {delta} = {new_val}"
            )
        self.set(path, new_val)
        return new_val

    def append(self, path: str, item: dict) -> str:
        """Append an item to a list at path. Auto-assigns item['id'] if missing. Returns id."""
        lst = self.get(path)
        if lst is None:
            self.set(path, [])
            lst = self.get(path)
        if not isinstance(lst, list):
            raise StateValueError(f"'{path}' is not a list — cannot append")
        if "id" not in item:
            item["id"] = str(uuid.uuid4())
        lst.append(item)
        # "item" is stored as a deep copy so a later in-place mutation by the
        # caller can't retroactively alter what fold(events) will replay —
        # the mutation record must be exactly what was true at append time.
        self._mutations.append({
            "op": "append", "path": path, "action": "append",
            "item_id": item["id"], "item": copy.deepcopy(item),
        })
        return item["id"]

    def remove(self, path: str, item_id: str) -> None:
        """Remove an item from a list by its 'id' field."""
        lst = self.get_strict(path)
        if not isinstance(lst, list):
            raise StateValueError(f"'{path}' is not a list — cannot remove")
        before = len(lst)
        lst[:] = [item for item in lst if item.get("id") != item_id]
        if len(lst) == before:
            raise StatePathError(f"Item id='{item_id}' not found in list at '{path}'")
        self._mutations.append({"op": "remove", "path": path, "action": "remove", "item_id": item_id})

    def snapshot(self) -> dict:
        """Return a deep copy of the current state — read-only reference."""
        return copy.deepcopy(self._data)

    def mutations(self) -> list[dict]:
        """Return all mutations made since construction."""
        return list(self._mutations)

    def diff(self) -> dict:
        """Return a summary of changes from original to current state."""
        return {
            "original": self._original,
            "current": self._data,
            "mutations": self._mutations,
        }
