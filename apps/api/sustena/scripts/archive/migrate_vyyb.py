#!/usr/bin/env python3
"""
sustena/scripts/migrate_vyyb.py
────────────────────────────────────────────────────────────────────────────────
Epic 1.5.1 — Vyyb OS → Sustena XII migration script.

Reads the legacy Vyyb OS SQLite database (24-table standalone ledger) and
produces a Vyyb Biashara sustain seed file at:

    sustena/sustains/seeds/vyyb_seed.json

The seed JSON can be used to initialise a Vyyb sustain instance in Sustena XII:
    sustena.core.sustain_engine.SustainEngine.create_from_seed(seed)

Domain mappings
───────────────
  items                 → state.inventory.items       (keyed by item_id)
  inventory_movements   → state.inventory.movements   (array, all columns)
  recipes + recipe_lines → state.recipes.library      (recipe_id key, nested ingredients)
  orders + order_items  → state.orders.history        (order_id key, nested items)
  accounts              → state.accounts.chart        (account_id key)
  journal_entries +
  journal_lines         → state.accounts.journal_entries (with nested lines array)
                          + state.accounts.journal_lines (flat denorm array)
  parties               → state.parties               (party_id key)
  assets                → state.assets                (asset_id key)
  production_batches    → state.production.batches    (array)

Usage
─────
  python3 sustena/scripts/migrate_vyyb.py
  python3 sustena/scripts/migrate_vyyb.py --db /path/to/vyyb.db
  python3 sustena/scripts/migrate_vyyb.py --db /path/to/vyyb.db --stdout

  --db      Path to the legacy Vyyb OS SQLite database.
            Default: C:/Users/DELL/OneDrive/Documents/New project/.claude/worktrees/goofy-cohen-11f0d8/data/vyyb.db
  --stdout  Print JSON to stdout instead of writing seed file.
  --seed-dir Directory to write vyyb_seed.json into.
            Default: sustena/sustains/seeds/ (relative to script location)
"""

import argparse
import json
import pathlib
import sqlite3
import sys
from datetime import datetime

# ── Defaults ──────────────────────────────────────────────────────────────────

DEFAULT_DB_PATH = (
    r"C:\Users\DELL\OneDrive\Documents\New project\.claude\worktrees"
    r"\goofy-cohen-11f0d8\data\vyyb.db"
)

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
SEED_DIR_DEFAULT = SCRIPT_DIR.parent / "sustains" / "seeds"


# ── Helpers ───────────────────────────────────────────────────────────────────

def rows_as_dicts(conn: sqlite3.Connection, table: str) -> list[dict]:
    """Fetch all rows from a table as a list of dicts. Returns [] if table absent."""
    try:
        cur = conn.execute(f"SELECT * FROM {table}")
        cols = [d[0] for d in cur.description]
        return [dict(zip(cols, row)) for row in cur.fetchall()]
    except sqlite3.OperationalError:
        return []


def table_exists(conn: sqlite3.Connection, table: str) -> bool:
    cur = conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name=?", (table,)
    )
    return cur.fetchone() is not None


# ── Domain migrators ──────────────────────────────────────────────────────────

def migrate_inventory(conn: sqlite3.Connection) -> tuple[dict, list, int, int]:
    """Returns (items_dict, movements_list, item_count, movement_count)."""
    items_rows = rows_as_dicts(conn, "items")
    movements_rows = rows_as_dicts(conn, "inventory_movements")

    items = {}
    for row in items_rows:
        key = str(row.get("item_id") or row.get("id", ""))
        if not key:
            continue
        items[key] = row

    return items, movements_rows, len(items), len(movements_rows)


def migrate_recipes(conn: sqlite3.Connection) -> tuple[dict, int]:
    """Returns (library_dict, recipe_count)."""
    recipes_rows = rows_as_dicts(conn, "recipes")
    lines_rows = rows_as_dicts(conn, "recipe_lines")

    # Index lines by recipe_id
    lines_by_recipe: dict[str, list] = {}
    for line in lines_rows:
        rid = str(line.get("recipe_id", ""))
        lines_by_recipe.setdefault(rid, []).append(line)

    library = {}
    for row in recipes_rows:
        rid = str(row.get("recipe_id") or row.get("id", ""))
        if not rid:
            continue
        recipe = dict(row)
        recipe["recipe_id"] = rid
        recipe["ingredients"] = lines_by_recipe.get(rid, [])
        library[rid] = recipe

    return library, len(library)


def migrate_orders(conn: sqlite3.Connection) -> tuple[list, int]:
    """Returns (history_list, order_count)."""
    orders_rows = rows_as_dicts(conn, "orders")
    items_rows = rows_as_dicts(conn, "order_items")

    items_by_order: dict[str, list] = {}
    for item in items_rows:
        oid = str(item.get("order_id", ""))
        items_by_order.setdefault(oid, []).append(item)

    history = []
    for row in orders_rows:
        oid = str(row.get("order_id") or row.get("id", ""))
        order = dict(row)
        order["order_id"] = oid
        order["items"] = items_by_order.get(oid, [])
        history.append(order)

    return history, len(history)


def migrate_accounts(conn: sqlite3.Connection) -> tuple[dict, list, list, int, int]:
    """Returns (chart, journal_entries, journal_lines, je_count, jl_count)."""
    acct_rows = rows_as_dicts(conn, "accounts")
    je_rows = rows_as_dicts(conn, "journal_entries")
    jl_rows = rows_as_dicts(conn, "journal_lines")

    chart = {}
    for row in acct_rows:
        aid = str(row.get("account_id") or row.get("id", ""))
        if aid:
            chart[aid] = row

    # Index lines by entry_id
    lines_by_entry: dict[str, list] = {}
    for line in jl_rows:
        eid = str(line.get("entry_id") or line.get("journal_entry_id", ""))
        lines_by_entry.setdefault(eid, []).append(line)

    journal_entries = []
    for row in je_rows:
        eid = str(row.get("entry_id") or row.get("id", ""))
        entry = dict(row)
        entry["entry_id"] = eid
        entry["lines"] = lines_by_entry.get(eid, [])
        journal_entries.append(entry)

    return chart, journal_entries, jl_rows, len(journal_entries), len(jl_rows)


def migrate_parties(conn: sqlite3.Connection) -> tuple[dict, int]:
    rows = rows_as_dicts(conn, "parties")
    parties = {}
    for row in rows:
        pid = str(row.get("party_id") or row.get("id", ""))
        if pid:
            parties[pid] = row
    return parties, len(parties)


def migrate_assets(conn: sqlite3.Connection) -> tuple[dict, int]:
    rows = rows_as_dicts(conn, "assets")
    assets = {}
    for row in rows:
        aid = str(row.get("asset_id") or row.get("id", ""))
        if aid:
            assets[aid] = row
    return assets, len(assets)


def migrate_production(conn: sqlite3.Connection) -> tuple[list, int]:
    rows = rows_as_dicts(conn, "production_batches")
    return rows, len(rows)


# ── Main ──────────────────────────────────────────────────────────────────────

def run_migration(db_path: str, seed_dir: pathlib.Path, to_stdout: bool) -> None:
    db_path_obj = pathlib.Path(db_path)

    if not db_path_obj.exists():
        print(f"[WARN] Database not found at: {db_path_obj}", file=sys.stderr)
        print("[WARN] Generating empty-state seed (no data migrated).", file=sys.stderr)
        conn = None
    else:
        print(f"[INFO] Opening database: {db_path_obj}", file=sys.stderr)
        conn = sqlite3.connect(str(db_path_obj))

    summary: dict[str, int] = {}

    if conn:
        items, movements, ni, nm = migrate_inventory(conn)
        library, nr = migrate_recipes(conn)
        order_history, no = migrate_orders(conn)
        chart, journal_entries, journal_lines, nje, njl = migrate_accounts(conn)
        parties, np_ = migrate_parties(conn)
        assets, na = migrate_assets(conn)
        production_batches, nb = migrate_production(conn)
        conn.close()

        summary = {
            "inventory.items": ni,
            "inventory.movements": nm,
            "recipes.library": nr,
            "orders.history": no,
            "accounts.chart": len(chart),
            "accounts.journal_entries": nje,
            "accounts.journal_lines": njl,
            "parties": np_,
            "assets": na,
            "production.batches": nb,
        }
    else:
        items = {}
        movements = []
        library = {}
        order_history = []
        chart = {}
        journal_entries = []
        journal_lines = []
        parties = {}
        assets = {}
        production_batches = []
        summary = {k: 0 for k in [
            "inventory.items", "inventory.movements", "recipes.library",
            "orders.history", "accounts.chart", "accounts.journal_entries",
            "accounts.journal_lines", "parties", "assets", "production.batches"
        ]}

    seed = {
        "sustain_id": "vyyb",
        "spec_id": "vyyb",
        "migrated_at": datetime.utcnow().isoformat() + "Z",
        "source_db": str(db_path),
        "migration_summary": summary,
        "state": {
            "inventory": {
                "items": items,
                "movements": movements,
            },
            "accounts": {
                "chart": chart,
                "journal_entries": journal_entries,
                "journal_lines": journal_lines,
            },
            "parties": parties,
            "orders": {
                "active": [],
                "history": order_history,
            },
            "purchases": {"active": [], "history": []},
            "purchase_orders": {"active": [], "history": []},
            "assets": assets,
            "expenses": [],
            "calendar": {"events": []},
            "analytics": {"period_reports": []},
            "recipes": {"library": library},
            "production": {
                "batches": production_batches,
                "plans": [],
            },
            "kds": {"tasks": [], "station_routes": {}},
            "staff": {
                "roster": {},
                "schedules": [],
                "shifts": [],
                "payroll": [],
                "hours": [],
            },
            "procurement": {"suppliers": {}, "po_history": []},
        },
    }

    # ── Print summary ──────────────────────────────────────────────────────────
    print("\n── Migration Summary ─────────────────────────────────────────", file=sys.stderr)
    total = 0
    for domain, count in summary.items():
        print(f"  {domain:<35} {count:>6} rows", file=sys.stderr)
        total += count
    print(f"  {'TOTAL':<35} {total:>6} rows", file=sys.stderr)
    print("──────────────────────────────────────────────────────────────\n", file=sys.stderr)

    # ── Output ─────────────────────────────────────────────────────────────────
    output_json = json.dumps(seed, indent=2, ensure_ascii=False, default=str)

    if to_stdout:
        print(output_json)
    else:
        seed_dir.mkdir(parents=True, exist_ok=True)
        out_path = seed_dir / "vyyb_seed.json"
        out_path.write_text(output_json, encoding="utf-8")
        print(f"[INFO] Seed file written to: {out_path}", file=sys.stderr)
        print(f"[INFO] Seed size: {out_path.stat().st_size:,} bytes", file=sys.stderr)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Migrate legacy Vyyb OS SQLite DB to a Sustena XII Vyyb seed file."
    )
    parser.add_argument(
        "--db",
        default=DEFAULT_DB_PATH,
        help=f"Path to the Vyyb OS SQLite database (default: {DEFAULT_DB_PATH})",
    )
    parser.add_argument(
        "--stdout",
        action="store_true",
        help="Print seed JSON to stdout instead of writing to file.",
    )
    parser.add_argument(
        "--seed-dir",
        default=str(SEED_DIR_DEFAULT),
        help=f"Directory to write vyyb_seed.json into (default: {SEED_DIR_DEFAULT})",
    )
    args = parser.parse_args()

    run_migration(
        db_path=args.db,
        seed_dir=pathlib.Path(args.seed_dir),
        to_stdout=args.stdout,
    )


if __name__ == "__main__":
    main()
