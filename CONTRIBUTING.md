# Contributing to Sustena

Thank you for your interest. Sustena is pre-1.0 and under active architectural change, so please **open an issue to discuss before starting significant work** — the layout and the engine implementation language are both moving (see [ADR-0001](docs/adr/0001-phase2-restructure-and-architecture.md)).

---

## Getting set up

Requires **Python 3.11+** and **Node 20+**.

```bash
cd apps/api
pip install -r requirements.lock   # exact pins — use this one
cp .env.example .env
python -m pytest tests/ -q         # everything should pass before you change anything
```

**Install from `requirements.lock`, not `requirements.txt`.** The lock holds exact, verified-green pins. `requirements.txt` declares the direct dependencies with `>=` ranges, so it resolves differently on different days. This suite is the correctness oracle for the Rust engine port, so it must run against an identical dependency set every time. CI installs the lock. Regeneration steps are in the lock file's own header.

Always run pytest as `python -m pytest`, never bare `pytest`, so the package path resolves.

The default `.env` runs the whole stack offline with a mock language-model client — no API keys, no spend.

---

## The rules that matter most

These are not style preferences. They are correctness properties the engine depends on, and a change that breaks one will be rejected even if the tests happen to pass.

**1 — State is the fold of its events.**

Never write state directly. Every real state change goes through an operator (which the engine records), or through the engine's explicit external-commit path. The invariant is:

```
rebuild_state(sustain_id) == get_state(sustain_id)
```

This must hold after *any* operation. Assert it in tests for anything that mutates state. There is a class of bug where an operator edits a nested object in place, the change persists to the cache, but no event records it — and the state silently stops being reproducible. Assume you will hit it; assert against it.

**2 — Every state change passes the gate.**

Do not add a second path that writes state without the enforcement check. If you need to commit something the operator registry cannot express, use the engine's external-commit entry point, which runs the same gate.

**3 — A refusal changes nothing.**

When the gate refuses, no state persists, no event appends, no partial write survives. Refusal is a normal, expected outcome carried in the response body — not an exception, and not an HTTP error.

**4 — No `eval`, no `exec`, no dynamic code execution.**

Constraints, predicates, parse rules, and placeholder resolution are all pattern-matching and typed AST walking. This is deliberate and non-negotiable. If you need a new capability in the expression language, extend the grammar and the typed evaluator.

**5 — Be honest in the interface.**

If a value is unknown, render it as unknown. Never fabricate a number, a default, or a plausible-looking placeholder to fill a gap. An empty state is a designed, legible empty state — not zeros pretending to be data.

---

## Tests

The suite is the safety net for the ongoing engine port, so it is held to a high bar.

- The full suite must pass before every commit. Do not defer a failure.
- New operators must be registered in the operator registry test's known-operator list.
- Anything that mutates state asserts the fold invariant above.
- Prefer a test that reproduces the real failure over one that asserts the fix.

```bash
cd apps/api
python -m pytest tests/ -q                    # all
python -m pytest tests/test_sustain_engine.py -v   # one file
```

---

## Commits and pull requests

Conventional commits:

```
feat(scope): description
fix(scope): description
chore(scope): description
test(scope): description
docs: description
```

Keep each change small and reviewable, with its own passing test run. Describe *what property* your change preserves or restores, not only what code moved.

---

## Architecture decisions

Anything that changes a boundary — persistence, the engine's public interface, how state commits, what ships in the open framework — belongs in an ADR under `docs/adr/` before it is implemented.

---

## Reporting a vulnerability

Do not open a public issue. See [SECURITY.md](SECURITY.md).

---

## License

By contributing, you agree that your contributions are licensed under the [Apache License, Version 2.0](LICENSE).
