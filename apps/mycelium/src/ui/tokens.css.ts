/**
 * The design tokens, type-locked — and the **breakpoints**.
 *
 * Values are lifted verbatim from `IO/design/Sustena_Mockups_MissionControl.html`.
 * vanilla-extract makes them a TypeScript object: a typo in a token name is a
 * compile error, not a silently-transparent colour at runtime.
 *
 * ★★ The contrast ramp is the CORRECTED one — the old `--text-muted`/`--text-dim`
 * failed WCAG AA badly enough to be a legibility bug, and the frozen app's own
 * fix is carried forward rather than re-introduced.
 *
 * ★★★ **Breakpoints are tokens, not per-screen media queries.** They live here
 * so only `src/ui` consumes them — `layout.css.ts` for arrangement and
 * `ui.css.ts` for the three places an instrument adjusts its own density — which is what makes the narrow-width behaviour something the whole
 * cockpit shares rather than something each screen re-decides. It is also the
 * groundwork for Orchie: a phone-first face on this same codebase needs
 * responsive *primitives*, not a second set of screens.
 */
import { createGlobalTheme } from "@vanilla-extract/css";

/** ★ Plain strings, because a media query needs a literal at build time. */
export const bp = {
  /** Below this the cockpit stops being two columns. */
  md: "screen and (max-width: 900px)",
  /** Phone. Rails collapse, density drops one step, nothing scrolls sideways. */
  sm: "screen and (max-width: 560px)",
} as const;

export const vars = createGlobalTheme(":root", {
  color: {
    // surfaces
    bgBase: "#0d0d0e",
    bgSurface: "#161617",
    bgRaised: "#1f1f21",
    bgOverlay: "#28282b",
    // borders
    border: "#2a2a2d",
    borderMid: "#38383c",
    borderLight: "#4a4a50",
    // text — the corrected ramp
    textPrimary: "#e8e4dc",
    textSecondary: "#aca8a0",
    textMuted: "#9c9891",
    textDim: "#726f69",
    // the two accents
    amber: "#E8A020",
    amberDim: "#c07818",
    amberGlow: "rgba(232,160,32,.13)",
    amberBorder: "rgba(232,160,32,.32)",
    teal: "#2ab8a0",
    tealGlow: "rgba(42,184,160,.12)",
    tealBorder: "rgba(42,184,160,.32)",
    // status
    ok: "#4caf80",
    warn: "#e8a020",
    danger: "#e05050",
    dangerGlow: "rgba(224,80,80,.12)",
    dangerBorder: "rgba(224,80,80,.34)",
    info: "#5a92e0",
  },
  font: {
    /** Prose, labels, names. */
    ui: "'Inter Tight', system-ui, sans-serif",
    /** ★ Every NUMBER and every identifier. Tabular figures do not jitter. */
    mono: "'DM Mono', ui-monospace, monospace",
  },
  space: {
    xs: "4px",
    sm: "8px",
    md: "12px",
    lg: "18px",
    xl: "28px",
  },
  radius: {
    sm: "2px",
    md: "3px",
  },
  /** The instrument type scale, so a size is chosen from a set rather than typed. */
  size: {
    micro: "9px",
    label: "10px",
    meta: "10.5px",
    body: "11.5px",
    value: "13px",
    lead: "14px",
    big: "26px",
  },
});
