/**
 * The engine, as the UI sees it.
 *
 * ★★ A thin pass-through over the **generated** bindings — deliberately not a
 * hand-written client. Everything typed here comes from `bindings.ts`, which
 * comes from the Rust in `src-tauri/src/dto.rs`. There is nowhere for the UI
 * and the engine to disagree about a shape.
 */
import {
  commands,
  type Branch,
  type ConstraintReading,
  type EconomyDto,
  type GateResult,
  type Holarchy,
  type JsonValue,
  type LogEntryDto,
  type OperatorDto,
  type Result,
  type SustainDto,
  type SustainSummary,
  type TemplateId,
  type WorldDto,
} from "../bindings";

export type {
  Branch,
  ConstraintReading,
  EconomyDto,
  OperatorDto,
  GateResult,
  Holarchy,
  JsonValue,
  LogEntryDto,
  SustainDto,
  SustainSummary,
  TemplateId,
  WorldDto,
};

/**
 * ★ Unwrap a generated `Result`, turning the error half into a thrown value.
 *
 * The generated bindings model a Rust `Result` as a tagged union. That is the
 * right wire shape, but a caller should not have to branch on it at every call
 * site — and crucially the errors these commands return are DISK failures, not
 * gate refusals. A refusal never travels this path: it arrives as a `GateResult`
 * with `verdict: "refused"`, which is a value, not an error.
 */
function unwrap<T>(r: Result<T, string>): T {
  if (r.status === "error") throw new Error(r.error);
  return r.data;
}

export const engine = {
  world: (): Promise<WorldDto> => commands.getWorld(),
  sustain: (id: string | null = null): Promise<SustainDto | null> => commands.getSustain(id),
  select: async (id: string): Promise<boolean> => unwrap(await commands.selectSustain(id)),
  create: async (
    id: string,
    label: string,
    template: TemplateId,
    parent: string | null,
  ): Promise<boolean> => unwrap(await commands.createSustain(id, label, template, parent)),
  constraints: (id: string): Promise<ConstraintReading[]> => commands.getConstraints(id),
  operators: (id: string): Promise<OperatorDto[]> => commands.getOperators(id),
  economy: (): Promise<EconomyDto> => commands.getEconomy(),
  setParameter: (name: string, value: number): Promise<GateResult> =>
    commands.setParameter(name, value),
  /** A hypothetical branch. Nothing is written. */
  simulate: (id: string, steps: [string, JsonValue][]): Promise<Branch | null> =>
    commands.simulate(id, steps),
  log: async (id: string): Promise<LogEntryDto[]> => unwrap(await commands.getLog(id)),
  /** `null` means there is no such Sustain — a different fact from a refusal. */
  run: async (
    sustainId: string,
    operator: string,
    params: JsonValue,
  ): Promise<GateResult | null> => unwrap(await commands.runOperator(sustainId, operator, params)),
};

/* ── reading real state, honestly ─────────────────────────────────────────
 * The engine's state is an untyped JSON document by design — a Sustain's shape
 * is DECLARED, not compiled in, so the UI cannot have a static type for it.
 * These readers therefore return `null` for absent rather than 0, so a missing
 * dimension renders as "—" and never as a confident zero.
 */

export type Pocket = {
  name: string;
  allocated: number;
  spent: number;
  left: number;
};

const num = (v: unknown): number | null => (typeof v === "number" ? v : null);

export function liquidBalance(state: unknown): number | null {
  const f = (state as Record<string, any>)?.finances;
  return num(f?.liquid?.balance);
}

export function pockets(state: unknown): Pocket[] {
  const raw = (state as Record<string, any>)?.finances?.pockets;
  if (!raw || typeof raw !== "object") return [];
  return Object.entries(raw as Record<string, any>).map(([name, p]) => {
    const allocated = num(p?.allocated) ?? 0;
    const spent = num(p?.spent) ?? 0;
    return { name, allocated, spent, left: allocated - spent };
  });
}

export const fmt = (n: number | null | undefined): string => {
  if (n === null || n === undefined) return "—";
  // Negative zero is a real f64 value and renders as "-0.00", which reads as a
  // number that moved. It did not.
  const v = Object.is(n, -0) ? 0 : n;
  return v.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
};

/** Children of a Sustain, from the composition links the registry holds. */
export const childrenOf = (world: WorldDto | undefined, id: string): SustainSummary[] =>
  (world?.sustains ?? []).filter((s) => s.parent === id);
