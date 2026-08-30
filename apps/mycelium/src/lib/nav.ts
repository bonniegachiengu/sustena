/**
 * RULE 2 — **navigable-looking implies navigable.**
 *
 * ★★★ **The dead end was structural, not a run of forgotten handlers.** The
 * cockpit had exactly one way to change the subject — `open(id)` in `App.tsx` —
 * and it was handed to exactly one screen, `Constellation`. Every other screen
 * received no navigation at all. So a linked child in Composition, a member in
 * Network, an account in Economy and a pocket in Monitor were not screens that
 * *forgot* to be clickable; they were screens with **nothing to click to**. No
 * amount of per-screen patching fixes that, because the thing missing was not
 * in the screens.
 *
 * ★★ **So navigation is ambient rather than threaded.** `App.tsx` registers
 * what it can do, once, and every screen can reach it without a prop chain
 * through components that do not care. A screen that lists a Sustain can send
 * a person to it in one line, which is the only way "navigable-looking implies
 * navigable" survives contact with the next screen somebody writes.
 *
 * ★★ **And the honest half.** {@link canOpenSustain} exists so a surface can
 * tell the difference between *somewhere to go* and *nowhere to go* — and
 * render them differently. An affordance that looks live and does nothing is
 * worse than a plain row: it teaches a person that this app's rows lie, and
 * they stop trying the ones that work. Where there is genuinely nowhere to go,
 * the row must look like a fact rather than a door.
 */
import { world } from "./live";

/** The cockpit's panels. Mirrors `App.tsx`'s own `Panel`. */
export type PanelId =
  | "constellation"
  | "monitor"
  | "council"
  | "console"
  | "simulate"
  | "define"
  | "composition"
  | "economy"
  | "ingest"
  | "network"
  | "library"
  | "profile";

type Handlers = {
  /** Select a Sustain AND open Monitor on it. */
  openSustain: (id: string) => void;
  /** Change panel, keeping the current subject. */
  goPanel: (panel: PanelId) => void;
};

/**
 * Unset until `App.tsx` registers.
 *
 * ★ No-ops rather than throws, deliberately: a screen rendered in a test or a
 * storybook has no cockpit around it, and a navigation call is not worth
 * failing a render for.
 */
let handlers: Handlers = {
  openSustain: () => {},
  goPanel: () => {},
};

export const registerNav = (h: Handlers) => {
  handlers = h;
};

/**
 * Is there really a Sustain behind this id?
 *
 * ★★★ The guard that keeps rule 2 honest in both directions. A row for a
 * Sustain the store does not hold — a stale link, a child on another node, an
 * id from a log line whose subject was since dissolved — has nowhere to go, and
 * a surface that renders it as a door is lying. Callers use this to decide
 * whether to offer the affordance at all, rather than offering one that
 * silently does nothing.
 */
export const canOpenSustain = (id: string | null | undefined): boolean =>
  !!id && !!world.sustains[id];

/** Go to a Sustain. Silent when there is nothing there — see `canOpenSustain`. */
export const openSustain = (id: string | null | undefined) => {
  if (canOpenSustain(id)) handlers.openSustain(id!);
};

/** Go to a panel, keeping the subject. */
export const goPanel = (panel: PanelId) => handlers.goPanel(panel);

/**
 * The click handler for a row that names a Sustain, or `undefined`.
 *
 * ★★ Returning `undefined` rather than a no-op is the whole point: `Row`
 * renders a button when it has a handler and a plain div when it does not, so
 * this single expression decides *both* the behaviour and the appearance. A
 * row cannot end up looking like a door and behaving like a wall, because one
 * value drives both.
 */
export const openRow = (id: string | null | undefined): (() => void) | undefined =>
  canOpenSustain(id) ? () => openSustain(id) : undefined;

/** The same, for a row whose job is to take somebody to a panel. */
export const panelRow = (panel: PanelId): (() => void) => () => goPanel(panel);
