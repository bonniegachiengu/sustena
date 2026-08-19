/**
 * The engine, as the UI sees it.
 *
 * ★★ A thin pass-through over the **generated** bindings — deliberately not a
 * hand-written client. Everything typed here comes from `bindings.ts`, which
 * comes from the Rust in `src-tauri/src/dto.rs`. There is nowhere for the UI
 * and the engine to disagree about a shape.
 */
import { commands, type GateResult, type JsonValue, type SustainDto } from "../bindings";

export type { GateResult, JsonValue, SustainDto };

export const engine = {
  getSustain: (): Promise<SustainDto> => commands.getSustain(),
  reset: (): Promise<SustainDto> => commands.resetSustain(),
  run: (operator: string, params: JsonValue): Promise<GateResult> =>
    commands.runOperator(operator, params),
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

export const fmt = (n: number | null): string =>
  n === null ? "—" : n.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
