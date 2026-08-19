/**
 * The three panels whose subsystem is not in `sustena-core`, plus Council and
 * Profile, which are.
 *
 * ★★ Each absence is authored: what exists, what does not, where it lives. The
 * wording is the report, not a placeholder.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import * as s from "../styles/app.css";
import { vars } from "../styles/tokens.css";
import { engine, type CouncilOutcomeDto } from "../lib/engine";
import { world } from "../lib/live";
import Unavailable from "./Unavailable";

/* ── Ingest ─────────────────────────────────────────────────────────────── */

export function Ingest() {
  return (
    <Unavailable
      absence={{
        title: "ingest · capture",
        headline:
          "There is no transducer in this engine. Nothing on this machine can turn an SMS, a bank alert or a narrated effect into a proposed operator call.",
        present: [
          "The gate, the fold and the log — anything ingest produced would land through exactly the path the Console already uses.",
          "Clock discipline: `clocks::skew_of` distinguishes an observed ingest time from an inferred one, and reports `Unknown` rather than guessing.",
        ],
        missing: [
          "The transducer — the deterministic parsers that read a real M-Pesa or KCB message into an amount, a counterparty and a direction.",
          "Declared `ParseRule` data, the correction-learning path, and the secret/OTP rejection filter that must run before anything is stored.",
          "Effect-first capture — inferring `(operator, θ)` from a plain-language effect.",
        ],
        where:
          "All of it is in the Python engine (`transducer.py`, `parse_rule.py`, `effect_capture.py`) and none of it is ported to `sustena-core`. A grep of the core for transducer/parse-rule/capture returns nothing; the single `ingest` hit is `clocks.rs`'s `t_ingest`, an unrelated sense of the word.",
        refusal:
          "A host-side parser is not built here on purpose. The Python transducer's own history is a sequence of speculative patterns that matched no real message until real samples arrived — writing a fresh guess in this app would repeat exactly that mistake, and a wrong parse of a financial message is a wrong entry in a ledger.",
      }}
    />
  );
}

/* ── Network ────────────────────────────────────────────────────────────── */

export function Network() {
  return (
    <Unavailable
      absence={{
        title: "network · nodes",
        headline:
          "This is a single node, and there is no peer transport. The distributed machinery exists in the engine; the connection does not.",
        present: [
          "CRDTs with proven convergence laws — `GCounter`, `PnCounter`, `OrSet`, `Rga`, plus `converges` and `laws_hold` as real checks.",
          "Vector clocks (`vclock`) and causal stamps for ordering without a shared clock.",
          "Paxos-style consensus — `propose`, `Promise`, `Accepted`, `Decision`, with a declared `ByzantineBound`.",
          "A `Router` with reinforcement and a `Division` model for assigning roles across principals.",
        ],
        missing: [
          "Any transport. There is no socket, no peer discovery and no gossip — by ADR-0001 the core has no I/O at all, and this host has not added one.",
          "A node identity beyond the local principal, and therefore no peer list to show.",
        ],
        where:
          "The primitives are in `sustena-core` today and are genuinely tested. What is absent is the host-side layer that would carry them between machines — the Axum/WebSocket half of the stack document's one-core-two-hosts diagram.",
        refusal:
          "A peer list showing `0` would imply a network that found nobody. There is no network. The distinction matters when the next question is *why has nothing synced*.",
      }}
    />
  );
}

/* ── Library ────────────────────────────────────────────────────────────── */

export function Library() {
  return (
    <Unavailable
      absence={{
        title: "library · arena",
        headline:
          "There is no package registry in this engine. Nothing here can publish, browse or install a shared operator, operative or widget.",
        present: [
          "The pieces a package would carry: `OperatorMeta` with declared params and protocol, `WidgetDecl` with a real typecheck, `StrategyGraph`, and `MemeLibrary` with provenance-scoped trust.",
          "The economy that would price a contribution — `royalty::split` with the ratified five-way schedule, settling as a conserved internal transfer.",
        ],
        missing: [
          "The Arena itself: packages, trust scores, downloads, orders — none of it is in the core.",
          "Any notion of a remote source to fetch from, which is the transport gap the Network panel names.",
        ],
        where:
          "Arena lives in the Python app (`routes/arena.py`, `arena_packages`, `arena_orders`). Porting it is a distribution question as much as an engine one, and it depends on the transport that does not exist yet.",
      }}
    />
  );
}

/* ── Council — real ─────────────────────────────────────────────────────── */

type Row = { operative: string; choice: string; confidence: number };

const COUNCILLORS = ["Mentor", "Protégé", "Attaché", "Curator", "Navigator"];

export function Council() {
  const [rows, setRows] = createSignal<Row[]>(
    COUNCILLORS.map((c, i) => ({
      operative: c,
      choice: i === 0 ? "yes" : i === 1 ? "yes" : "abstain",
      confidence: 0.7,
    })),
  );
  const [userVote, setUserVote] = createSignal<string>("none");
  const [collected, setCollected] = createSignal(true);
  const [outcome, setOutcome] = createSignal<CouncilOutcomeDto | null>(null);
  const [busy, setBusy] = createSignal(false);

  const run = async () => {
    setBusy(true);
    try {
      setOutcome(
        await engine.resolveProposal(
          rows().map((r) => [r.operative, r.choice, r.confidence] as [string, string, number]),
          userVote() === "none" ? null : userVote(),
          collected(),
        ),
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class={s.main}>
      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>council · deliberation</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>{rows().length} councillors</span>
          </header>
          <div class={s.cardBody}>
            <For each={rows()}>
              {(r, i) => (
                <div class={s.paramRow}>
                  <span class={s.value}>{r.operative}</span>
                  <select
                    class={s.picker}
                    value={r.choice}
                    onChange={(e) =>
                      setRows((v) =>
                        v.map((x, j) => (j === i() ? { ...x, choice: e.currentTarget.value } : x)),
                      )
                    }
                  >
                    <option value="yes">yes</option>
                    <option value="no">no</option>
                    <option value="abstain">abstain</option>
                  </select>
                  <input
                    class={s.input}
                    style={{ width: "68px" }}
                    value={String(r.confidence)}
                    onInput={(e) =>
                      setRows((v) =>
                        v.map((x, j) =>
                          j === i() ? { ...x, confidence: Number(e.currentTarget.value) || 0 } : x,
                        ),
                      )
                    }
                  />
                </div>
              )}
            </For>

            <div class={s.paramRow}>
              <span class={s.value}>the person</span>
              <select
                class={s.picker}
                value={userVote()}
                onChange={(e) => setUserVote(e.currentTarget.value)}
              >
                <option value="none">has not decided</option>
                <option value="yes">yes</option>
                <option value="no">no</option>
                <option value="abstain">abstain</option>
              </select>
              <button class={s.chip} onClick={() => setCollected((c) => !c)}>
                {collected() ? "collected" : "not collected"}
              </button>
            </div>

            <div style={{ "margin-top": vars.space.lg }}>
              <button class={s.button} onClick={run} disabled={busy()}>
                {busy() ? "resolving…" : "resolve"}
              </button>
            </div>
          </div>
        </section>
      </div>

      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>the engine's resolution</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={outcome()}
              fallback={<div class={s.empty}>nothing resolved yet</div>}
            >
              {(o) => (
                <>
                  <div class={s.row}>
                    <span class={s.label}>status</span>
                    <span class={s.valueBig}>{o().status}</span>
                  </div>
                  <div class={s.row}>
                    <span class={s.label}>aggregated vote</span>
                    <span class={s.value}>{o().aggregated}</span>
                  </div>
                  <div class={s.row}>
                    <span class={s.label}>utility</span>
                    <span class={s.value}>{o().utility.toFixed(2)}</span>
                  </div>
                  <Show when={o().reasoning.trim()}>
                    <p class={s.reason}>{o().reasoning}</p>
                  </Show>
                </>
              )}
            </Show>

            <div class={s.attentionWhy} style={{ "margin-top": vars.space.md }}>
              `council::resolve` and `aggregate_delegated_votes` are the engine's own. The rules
              they encode — a person's vote overrides the council, an <em>abstaining</em> person
              leaves it <strong>in voting</strong> rather than deciding, and collected votes with
              nobody in favour fail — are not re-implemented here.
            </div>
          </div>
        </section>

        <section class={s.card}>
          <div class={s.cardBody}>
            <div class={s.unavailable}>
              <span class={s.unavailableTitle}>what is not here</span>
              Resolution is real; the <strong>proposal lifecycle</strong> is not. There is no
              persisted proposal, no deadline (the core compares nothing it cannot replay, so
              `expired` is the host's question and this app has none), and no operative actually
              deliberating — the votes above are yours to set, so this is the engine's rule engine
              exercised by hand rather than a council that met.
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
