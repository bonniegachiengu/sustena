/**
 * The live store — **real-time first**.
 *
 * ★★★ Two subscriptions, one store, and every screen reads from it. There is no
 * polling and no re-fetch on change: the host pushes on every gate decision, so
 * the store is correct after each message without asking the engine anything.
 *
 * ★★ Fine-grained on purpose. Solid's `createStore` diffs by path, so a message
 * that moves one pocket's `spent` repaints the cells that read `spent` and
 * nothing else — no virtual DOM, no tree walk, no reconciliation of six
 * habitats because one of them changed. This is the pattern the rest of the
 * cockpit is meant to reuse: **subscribe once, merge surgically, read reactively.**
 *
 * ★★ **Two events, because they answer two questions.** `Committed` says *what
 * changed* and carries the new state. `Refused` says *what was asked and
 * declined* and carries **no state at all** — so a refusal reaches the gate
 * stream and touches nothing else. A screen driven by this store cannot show a
 * refusal as a state change, because there is no state on the message to show.
 */
import { createStore, produce, reconcile } from "solid-js/store";
import { events, type Committed, type Refused, type RollupDto } from "../bindings";
import { engine, type ConstraintReading, type IdentityDto, type LogEntryDto, type SustainSummary, type WorldDto } from "./engine";

export type { Committed, ConstraintReading, LogEntryDto, Refused, RollupDto };

/**
 * One line of the gate stream.
 *
 * The two kinds come from two different events, and stay different here.
 * `admitted` carries a seq — it is a line in a log that exists. `refused`
 * carries none, because there is no line: nothing was written.
 */
export type StreamEntry =
  | { kind: "admitted"; at: number; sustainId: string; operator: string; seq: number; events: string[] }
  | { kind: "refused"; at: number; sustainId: string; operator: string; reason: string; rule: string };

/** How many gate-stream lines to keep. Older ones fall off the end. */
const STREAM_CAP = 60;

/** What one Sustain looks like to a live screen. */
export type LiveSustain = {
  summary: SustainSummary;
  /** The engine's state document. Untyped by design — a Sustain's shape is declared, not compiled in. */
  state: unknown;
  constraints: ConstraintReading[];
  log: LogEntryDto[];
  /**
   * ρ, as the engine last computed it — `null` until it has.
   *
   * ★ Held per Sustain rather than globally: a household total belongs to the
   * household, and a cockpit that can select any Sustain needs to know whose
   * total it is looking at.
   */
  rollup: RollupDto | null;
};

type LiveWorld = {
  loaded: boolean;
  error: string | null;
  storePath: string;
  selected: string | null;
  order: string[];
  holds: boolean;
  holarchyReason: string;
  linked: number;

  sustains: Record<string, LiveSustain>;
  principal: string;
  /**
   * ★★★ Who this machine is, and whether the key is unlocked. The whole
   * cockpit is behind this: `loaded` is never reached while locked, because
   * there is nothing a locked world can honestly show.
   */
  identity: IdentityDto | null;
  /** ★ Counts pushes received. The number a proof can point at. */
  pushes: number;
  lastPush: Committed | null;
  /** ★★★ The live gate stream — newest first. Fed ONLY by the channel. */
  stream: StreamEntry[];
  refusals: number;
};

const [world, setWorld] = createStore<LiveWorld>({
  loaded: false,
  error: null,
  storePath: "",
  selected: null,
  order: [],
  holds: false,
  holarchyReason: "",
  linked: 0,
  sustains: {},
  principal: "",
  identity: null,
  pushes: 0,
  lastPush: null,
  stream: [],
  refusals: 0,
});

export { world };

function mergeWorld(w: WorldDto) {
  setWorld(
    produce((s) => {
      s.loaded = true;
      s.error = null;
      s.storePath = w.storePath;
      s.principal = w.principal;
      s.selected = w.selected;
      s.order = w.sustains.map((x) => x.id);
      if (w.holarchy.kind === "holds") {
        s.holds = true;
        s.linked = w.holarchy.linked;
        s.holarchyReason = "";
      } else {
        s.holds = false;
        s.linked = 0;
        s.holarchyReason = w.holarchy.reason;
      }
      for (const summary of w.sustains) {
        const existing = s.sustains[summary.id];
        if (existing) {
          existing.summary = summary;
        } else {
          s.sustains[summary.id] = {
            summary,
            state: null,
            constraints: [],
            log: [],
            rollup: null,
          };
        }
      }
    }),
  );
}

/** Pull the parts a push does not carry — the log, and any Sustain's state we have not seen. */
export async function hydrate(id: string): Promise<void> {
  try {
    const [full, constraints, log, rollup] = await Promise.all([
      engine.sustain(id),
      engine.constraints(id),
      engine.log(id),
      // ★ ρ on hydrate only. After that the channel carries it — a total that
      //   re-read itself on a timer would be polling for a number the engine
      //   already pushed.
      engine.rollup(id),
    ]);
    setWorld(
      produce((s) => {
        const entry = s.sustains[id];
        if (!entry) return;
        entry.state = full?.state ?? null;
        entry.constraints = constraints;
        entry.log = log;
        entry.rollup = rollup;
      }),
    );
  } catch (e) {
    setWorld("error", String(e));
  }
}

/** Re-read the identity. ★ The only thing a locked cockpit may ask for. */
export async function refreshIdentity(): Promise<IdentityDto | null> {
  try {
    const id = await engine.identity();
    setWorld("identity", id);
    return id;
  } catch (e) {
    setWorld(produce((s) => {
      s.error = String(e);
    }));
    return null;
  }
}

/** Read the world once. ★ Called on mount and after a selection change — never on a push. */
export async function refreshWorld(): Promise<void> {
  try {
    mergeWorld(await engine.world());
  } catch (e) {
    setWorld(produce((s) => {
      s.error = String(e);
      s.loaded = true;
    }));
  }
}

/**
 * ★★★ Subscribe to the push channel.
 *
 * Everything below happens **without asking the engine anything**. The message
 * carries the new state, the events it published and `V` re-evaluated, so the
 * merge is complete — and the log entry is appended from the message's own
 * fields rather than re-read from disk.
 */
export async function subscribe(): Promise<() => void> {
  const stopCommitted = await events.committed.listen((e) => {
    const m = e.payload;
    setWorld(
      produce((s) => {
        s.pushes += 1;
        s.lastPush = m;

        // The gate stream. Newest first, capped -- a ticker that grew without
        // bound would be a memory leak wearing a feature's name.
        s.stream = [
          {
            kind: "admitted" as const,
            at: Date.now(),
            sustainId: m.sustainId,
            operator: m.operator,
            seq: m.seq,
            events: m.events.map((x) => x.name),
          },
          ...s.stream,
        ].slice(0, STREAM_CAP);

        const entry = s.sustains[m.sustainId];
        if (!entry) return;
        // `reconcile` keeps identity for untouched sub-paths, so only the
        // values that actually moved re-render.
        entry.state = reconcile(m.state as object, { merge: true })(entry.state as object);
        entry.constraints = m.constraints;
        // The summary is updated FROM THE MESSAGE -- pockets included -- so the
        // constellation is live without hydrating anything.
        entry.summary = {
          ...entry.summary,
          liquid: m.liquid,
          events: m.seq + 1,
          constraints: m.constraints,
          pockets: pocketsFromState(m.state),
        };
        entry.log = [
          ...entry.log,
          { seq: m.seq, operator: m.operator, events: m.events, mutations: m.mutations },
        ];
      }),
    );
  });

  // A refusal changes no state, so it touches NOTHING but the stream. There is
  // no `state` on the message to merge even if this wanted to.
  const stopRefused = await events.refused.listen((e) => {
    const m = e.payload;
    setWorld(
      produce((s) => {
        s.refusals += 1;
        s.stream = [
          {
            kind: "refused" as const,
            at: Date.now(),
            sustainId: m.sustainId,
            operator: m.operator,
            reason: m.reason ?? "",
            rule: m.constraintViolated ?? "",
          },
          ...s.stream,
        ].slice(0, STREAM_CAP);
      }),
    );
  });

  // ★★★ ρ, pushed. A member's commit moves its HOUSEHOLD's total, so this
  //     message names a different Sustain from the one that changed — which is
  //     exactly why it is its own event rather than a field on `Committed`.
  //     Nothing here asks the engine anything; the total arrives computed.
  const stopRolledUp = await events.rolledUp.listen((e) => {
    const r = e.payload.rollup;
    setWorld(
      produce((s) => {
        const entry = s.sustains[r.sustainId];
        if (!entry) return;
        entry.rollup = r;
      }),
    );
  });

  return () => {
    stopCommitted();
    stopRefused();
    stopRolledUp();
  };
}

/** Pockets at summary scale, read from a pushed state document. */
function pocketsFromState(state: unknown): { name: string; allocated: number; spent: number }[] {
  const raw = (state as Record<string, any>)?.finances?.pockets;
  if (!raw || typeof raw !== "object") return [];
  return Object.entries(raw as Record<string, any>).map(([name, p]) => ({
    name,
    allocated: typeof p?.allocated === "number" ? p.allocated : 0,
    spent: typeof p?.spent === "number" ? p.spent : 0,
  }));
}

/* ── derived readings ──────────────────────────────────────────────────── */

export const selectedSustain = (): LiveSustain | undefined =>
  world.selected ? world.sustains[world.selected] : undefined;

export const childrenOf = (id: string): SustainSummary[] =>
  world.order
    .map((k) => world.sustains[k])
    .filter((x): x is LiveSustain => !!x && x.summary.parent === id)
    .map((x) => x.summary);

/* -- attention, across every Sustain -------------------------------------- */

/**
 * The threshold is THIS APP'S declared policy, not an engine reading, and every
 * surface that uses it says so. The engine's own urgency is `d(s,V)` over a
 * declared `Region`; these templates declare invariants rather than intervals,
 * so borrowing a Region's authority would mean declaring thresholds nobody
 * chose.
 */
export const ATTENTION_AT = 0.8;

export type Attention = {
  sustainId: string;
  label: string;
  kind: "rule" | "pocket";
  what: string;
  why: string;
  severity: "warn" | "danger";
};

/** Everything that wants a person, across the whole household. */
export function attentionAcross(): Attention[] {
  const out: Attention[] = [];
  for (const id of world.order) {
    const entry = world.sustains[id];
    if (!entry) continue;
    const summary = entry.summary;
    // The engine's verdict first -- a broken rule outranks a strained pocket.
    for (const c of summary.constraints) {
      if (!c.holds) {
        out.push({
          sustainId: id,
          label: summary.label,
          kind: "rule",
          what: c.id,
          why: c.reason || c.expression,
          severity: "danger",
        });
      }
    }
    for (const p of summary.pockets) {
      if (p.allocated <= 0) continue;
      const f = p.spent / p.allocated;
      if (f >= ATTENTION_AT) {
        out.push({
          sustainId: id,
          label: summary.label,
          kind: "pocket",
          what: p.name,
          why: `${Math.round(f * 100)}% spent - ${(p.allocated - p.spent).toFixed(2)} left`,
          severity: f >= 1 ? "danger" : "warn",
        });
      }
    }
  }
  return out;
}

/** Whether one Sustain wants attention -- drives its node dot. */
export function attentionFor(id: string): "ok" | "warn" | "danger" {
  const hits = attentionAcross().filter((a) => a.sustainId === id);
  if (hits.some((h) => h.severity === "danger")) return "danger";
  if (hits.length > 0) return "warn";
  return "ok";
}
