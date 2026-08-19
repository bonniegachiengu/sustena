/**
 * The live store — **real-time first**.
 *
 * ★★★ One subscription, `events.committed`, and every screen reads from the
 * store it fills. There is no polling and no re-fetch on change: the host
 * pushes exactly one message per committed change, so the store is correct
 * after each message without asking the engine anything.
 *
 * ★★ Fine-grained on purpose. Solid's `createStore` diffs by path, so a message
 * that moves one pocket's `spent` repaints the cells that read `spent` and
 * nothing else — no virtual DOM, no tree walk, no reconciliation of six
 * habitats because one of them changed. This is the pattern the rest of the
 * cockpit is meant to reuse: **subscribe once, merge surgically, read reactively.**
 *
 * ★ A refusal never arrives here. The host emits nothing for it, because
 * nothing changed — so a screen driven by this store cannot show a refusal as a
 * state change. The verdict is the Console's business; this is the world's.
 */
import { createStore, produce, reconcile } from "solid-js/store";
import { events, type Committed } from "../bindings";
import { engine, type ConstraintReading, type LogEntryDto, type SustainSummary, type WorldDto } from "./engine";

export type { Committed, ConstraintReading, LogEntryDto };

/** What one Sustain looks like to a live screen. */
export type LiveSustain = {
  summary: SustainSummary;
  /** The engine's state document. Untyped by design — a Sustain's shape is declared, not compiled in. */
  state: unknown;
  constraints: ConstraintReading[];
  log: LogEntryDto[];
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
  rollupAvailable: boolean;
  sustains: Record<string, LiveSustain>;
  /** ★ Counts pushes received. The number a proof can point at. */
  pushes: number;
  lastPush: Committed | null;
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
  rollupAvailable: false,
  sustains: {},
  pushes: 0,
  lastPush: null,
});

export { world };

function mergeWorld(w: WorldDto) {
  setWorld(
    produce((s) => {
      s.loaded = true;
      s.error = null;
      s.storePath = w.storePath;
      s.selected = w.selected;
      s.order = w.sustains.map((x) => x.id);
      s.rollupAvailable = w.rollupAvailable;
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
          s.sustains[summary.id] = { summary, state: null, constraints: [], log: [] };
        }
      }
    }),
  );
}

/** Pull the parts a push does not carry — the log, and any Sustain's state we have not seen. */
export async function hydrate(id: string): Promise<void> {
  try {
    const [full, constraints, log] = await Promise.all([
      engine.sustain(id),
      engine.constraints(id),
      engine.log(id),
    ]);
    setWorld(
      produce((s) => {
        const entry = s.sustains[id];
        if (!entry) return;
        entry.state = full?.state ?? null;
        entry.constraints = constraints;
        entry.log = log;
      }),
    );
  } catch (e) {
    setWorld("error", String(e));
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
  return events.committed.listen((e) => {
    const m = e.payload;
    setWorld(
      produce((s) => {
        s.pushes += 1;
        s.lastPush = m;
        const entry = s.sustains[m.sustainId];
        if (!entry) return;
        // `reconcile` keeps identity for untouched sub-paths, so only the
        // values that actually moved re-render.
        entry.state = reconcile(m.state as object, { merge: true })(entry.state as object);
        entry.constraints = m.constraints;
        entry.summary = { ...entry.summary, liquid: m.liquid, events: m.seq + 1 };
        entry.log = [
          ...entry.log,
          { seq: m.seq, operator: m.operator, events: m.events, mutations: m.mutations },
        ];
      }),
    );
  });
}

/* ── derived readings ──────────────────────────────────────────────────── */

export const selectedSustain = (): LiveSustain | undefined =>
  world.selected ? world.sustains[world.selected] : undefined;

export const childrenOf = (id: string): SustainSummary[] =>
  world.order
    .map((k) => world.sustains[k])
    .filter((x): x is LiveSustain => !!x && x.summary.parent === id)
    .map((x) => x.summary);
