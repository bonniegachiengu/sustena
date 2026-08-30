/**
 * The cockpit's design system, as CSS.
 *
 * ★★★ **One definition each.** Before this, `app.css.ts` held 98 ad-hoc exports
 * grown a section per slice, and the same card / verdict / empty-state was
 * re-spelled on every screen. Everything a screen shares now lives here and is
 * consumed through the components in `index.tsx` — which is what makes this a
 * system rather than a pile of screens, and the seam a Storybook hangs off.
 *
 * ★★ **Arrangement lives in `layout.css.ts`, not here.** These are the
 * instruments; the primitives arrange them. The only media queries below are
 * the three an instrument makes about ITSELF — a big figure that would not fit
 * a phone, a card's padding, a control row that becomes one column. Anything
 * about how instruments sit next to each other belongs next door, so the
 * narrow-width behaviour is never something a screen re-decides.
 */
import { globalStyle, keyframes, style, styleVariants } from "@vanilla-extract/css";
import { bp, vars } from "./tokens.css";

/* ── the ground ─────────────────────────────────────────────────────────── */

globalStyle("*, *::before, *::after", { boxSizing: "border-box" });
globalStyle("html, body, #root", { height: "100%", margin: 0 });
globalStyle("body", {
  background: vars.color.bgBase,
  color: vars.color.textPrimary,
  fontFamily: vars.font.ui,
  fontSize: vars.size.value,
  WebkitFontSmoothing: "antialiased",
  // ★ The one rule that makes "no horizontal scroll" a property of the app
  //   rather than of each screen remembering to set `min-width: 0`.
  overflowX: "hidden",
});
globalStyle("::-webkit-scrollbar", { width: "6px", height: "6px" });
globalStyle("::-webkit-scrollbar-track", { background: "transparent" });
globalStyle("::-webkit-scrollbar-thumb", { background: vars.color.borderMid, borderRadius: "3px" });
globalStyle("code", { fontFamily: vars.font.mono, fontSize: "0.94em", color: vars.color.textPrimary });

/* ── typography ─────────────────────────────────────────────────────────── */

/** An uppercase mono label. The cockpit's only label style. */
export const label = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.label,
  fontWeight: 500,
  letterSpacing: "0.10em",
  textTransform: "uppercase",
  color: vars.color.textMuted,
});

/**
 * ★ A symbol that must NOT be case-folded. `text-transform: uppercase` turns ρ
 * into Ρ (capital Rho) — a different letter, and the cockpit's labels are full
 * of Greek that carries meaning. One definition, so a title can be uppercase
 * and still say ρ.
 */
export const sym = style({ textTransform: "none" });

/** Secondary mono — identifiers, expressions, small facts. */
export const meta = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.body,
  color: vars.color.textSecondary,
  minWidth: 0,
  overflowWrap: "anywhere",
});

/** The dimmest readable tier — reasons, captions, the "why". */
export const caption = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.meta,
  lineHeight: 1.55,
  color: vars.color.textMuted,
  overflowWrap: "anywhere",
});

/** Prose. The only non-mono body style. */
export const prose = style({
  fontFamily: vars.font.ui,
  fontSize: vars.size.lead,
  lineHeight: 1.55,
  color: vars.color.textPrimary,
  margin: 0,
});

export const value = styleVariants({
  base: [
    {
      fontFamily: vars.font.mono,
      fontSize: vars.size.value,
      fontWeight: 500,
      color: vars.color.textPrimary,
      fontVariantNumeric: "tabular-nums",
    },
  ],
  big: [
    {
      fontFamily: vars.font.mono,
      fontSize: vars.size.big,
      fontWeight: 500,
      color: vars.color.textPrimary,
      letterSpacing: "-0.01em",
      fontVariantNumeric: "tabular-nums",
      "@media": { [bp.sm]: { fontSize: "20px" } },
    },
  ],
});

/* ── card · the instrument frame ────────────────────────────────────────── */

export const card = style({
  border: `1px solid ${vars.color.border}`,
  background: vars.color.bgSurface,
  borderRadius: vars.radius.md,
  minWidth: 0,
  // ★★★ **`min-height: 0`, and it is not decoration.** A flex column defaults
  //     to `min-height: auto`, which refuses to shrink below its content — so
  //     a `Card scroll` grew to fit its body and the body never scrolled,
  //     however much overflow there was. The exact `min-width: 0` trap, one
  //     axis over, and the reason a tall card clipped instead of scrolling.
  minHeight: 0,
  display: "flex",
  flexDirection: "column",
  "@media": {
    // ★ A card is a flex item in the stacked column below `md`, and flex items
    //   shrink by default. Measured at 375px, a card whose content needed
    //   114px was being rendered at 22 and the rest of it drawn over the next
    //   card. A card is never shorter than what is in it.
    [bp.md]: { flexShrink: 0 },
  },
});

export const cardHead = style({
  display: "flex",
  alignItems: "center",
  gap: vars.space.sm,
  padding: `${vars.space.sm} ${vars.space.md}`,
  borderBottom: `1px solid ${vars.color.border}`,
  minWidth: 0,
  flexWrap: "wrap",
});

export const cardTitle = style([label, { whiteSpace: "nowrap" }]);

export const cardBody = style({
  padding: vars.space.md,
  minWidth: 0,
  minHeight: 0,
  "@media": { [bp.sm]: { padding: vars.space.sm } },
});

/** For a card whose body is the scrolling part. */
export const cardBodyScroll = style([cardBody, { overflowY: "auto" }]);

export const spacer = style({ flex: 1 });

/**
 * ★ A span that is ALLOWED TO SHRINK. Grid and flex children default to
 * `min-width: auto`, which refuses to go below their content and is the single
 * most common cause of a page scrolling sideways. One definition, so no screen
 * has to remember it.
 */
export const fill = style({ minWidth: 0 });

/** A block set off from what precedes it. Three steps, chosen — not typed. */
export const note = styleVariants({
  sm: [{ marginTop: vars.space.sm, minWidth: 0 }],
  md: [{ marginTop: vars.space.md, minWidth: 0 }],
  lg: [{ marginTop: vars.space.lg, minWidth: 0 }],
});

/* ── the honest-state vocabulary ────────────────────────────────────────── */

/**
 * ★★ Every "nothing here" in the cockpit renders through this. One style, one
 * voice — lowercase, quiet, never an illustration and never a call to action.
 */
export const emptyLine = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.body,
  color: vars.color.textMuted,
  padding: `${vars.space.md} 0`,
  overflowWrap: "anywhere",
});

export const errorBox = style({
  border: `1px solid ${vars.color.dangerBorder}`,
  background: vars.color.dangerGlow,
  borderRadius: vars.radius.md,
  padding: vars.space.md,
  fontFamily: vars.font.mono,
  fontSize: vars.size.body,
  lineHeight: 1.55,
  color: vars.color.textPrimary,
  overflowWrap: "anywhere",
});

/** A designed absence — dashed, never solid: it is a hole, not a panel. */
export const absence = style({
  border: `1px dashed ${vars.color.borderMid}`,
  borderRadius: vars.radius.md,
  padding: vars.space.md,
  fontFamily: vars.font.mono,
  fontSize: vars.size.meta,
  lineHeight: 1.6,
  color: vars.color.textMuted,
  overflowWrap: "anywhere",
});

export const absenceTitle = style([label, { display: "block", marginBottom: vars.space.xs }]);

/** A hypothetical is never allowed to look like a fact. */
export const hypothetical = style([
  absence,
  { borderColor: vars.color.amberBorder, background: vars.color.amberGlow, color: vars.color.textSecondary },
]);

/** The permanent economy boundary — solid, because it is a standing fact. */
export const boundary = style({
  border: `1px solid ${vars.color.borderMid}`,
  background: vars.color.bgRaised,
  borderRadius: vars.radius.md,
  padding: vars.space.md,
  fontFamily: vars.font.mono,
  fontSize: vars.size.meta,
  lineHeight: 1.6,
  color: vars.color.textSecondary,
});

/* ── badges + dots ──────────────────────────────────────────────────────── */

const badgeBase = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.label,
  fontWeight: 500,
  letterSpacing: "0.08em",
  borderRadius: vars.radius.sm,
  padding: `2px ${vars.space.sm}`,
  whiteSpace: "nowrap",
});

export const badge = styleVariants({
  ok: [badgeBase, { color: vars.color.teal, border: `1px solid ${vars.color.tealBorder}`, background: vars.color.tealGlow }],
  warn: [badgeBase, { color: vars.color.amber, border: `1px solid ${vars.color.amberBorder}`, background: vars.color.amberGlow }],
  danger: [badgeBase, { color: vars.color.danger, border: `1px solid ${vars.color.dangerBorder}`, background: vars.color.dangerGlow }],
  /** For a capability the engine does not have. Dashed, like every absence. */
  absent: [badgeBase, { color: vars.color.textMuted, border: `1px dashed ${vars.color.borderMid}` }],
  quiet: [badgeBase, { color: vars.color.textMuted, border: "1px solid transparent" }],
});

export const dot = styleVariants({
  ok: [{ width: "6px", height: "6px", borderRadius: "50%", display: "inline-block", flexShrink: 0, background: vars.color.teal }],
  warn: [{ width: "6px", height: "6px", borderRadius: "50%", display: "inline-block", flexShrink: 0, background: vars.color.warn }],
  danger: [{ width: "6px", height: "6px", borderRadius: "50%", display: "inline-block", flexShrink: 0, background: vars.color.danger }],
  idle: [{ width: "6px", height: "6px", borderRadius: "50%", display: "inline-block", flexShrink: 0, background: vars.color.borderLight }],
  active: [{ width: "6px", height: "6px", borderRadius: "50%", display: "inline-block", flexShrink: 0, background: vars.color.amber }],
});

/* ── the verdict — one definition, three screens ────────────────────────── */

const flash = keyframes({ "0%": { opacity: 0.35 }, "100%": { opacity: 1 } });

const verdictBase = style({
  border: `1px solid ${vars.color.border}`,
  borderRadius: vars.radius.md,
  padding: vars.space.md,
  minWidth: 0,
  animation: `${flash} 220ms ease-out`,
});

export const verdictBox = styleVariants({
  admitted: [verdictBase, { borderColor: vars.color.tealBorder, background: vars.color.tealGlow }],
  refused: [verdictBase, { borderColor: vars.color.dangerBorder, background: vars.color.dangerGlow }],
  deferred: [verdictBase, { borderColor: vars.color.amberBorder, background: vars.color.amberGlow }],
});

const verdictWordBase = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.value,
  fontWeight: 500,
  letterSpacing: "0.16em",
  whiteSpace: "nowrap",
});

export const verdictWord = styleVariants({
  admitted: [verdictWordBase, { color: vars.color.teal }],
  refused: [verdictWordBase, { color: vars.color.danger }],
  deferred: [verdictWordBase, { color: vars.color.amber }],
});

/** The engine's own words. Never re-phrased. */
export const reason = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.body,
  lineHeight: 1.55,
  color: vars.color.textPrimary,
  margin: `${vars.space.sm} 0 0`,
  overflowWrap: "anywhere",
});

export const reasonCode = style([label, { marginTop: vars.space.xs, letterSpacing: "0.08em" }]);

/* ── rows ───────────────────────────────────────────────────────────────── */

/** label ↔ value, the cockpit's most common line. */
export const readout = style({
  display: "flex",
  alignItems: "baseline",
  justifyContent: "space-between",
  gap: vars.space.md,
  padding: `${vars.space.xs} 0`,
  minWidth: 0,
});

/** A bordered row that wraps rather than overflowing. */
export const row = style({
  display: "flex",
  flexWrap: "wrap",
  alignItems: "baseline",
  gap: vars.space.sm,
  padding: `${vars.space.sm} 0`,
  borderTop: `1px solid ${vars.color.border}`,
  minWidth: 0,
});

/** A row that is a button. Same geometry, hover, no chrome. */
export const rowButton = style([
  row,
  {
    width: "100%",
    textAlign: "left",
    background: "transparent",
    border: "none",
    borderTop: `1px solid ${vars.color.border}`,
    color: "inherit",
    font: "inherit",
    cursor: "pointer",
    selectors: { "&:hover": { background: vars.color.bgRaised } },
  },
]);

/** An icon-and-explanation line: dot, then a block of text that wraps. */
export const noteRow = style({
  display: "grid",
  gridTemplateColumns: "auto 1fr",
  gap: vars.space.sm,
  alignItems: "start",
  padding: `${vars.space.xs} 0`,
  minWidth: 0,
});

export const noteDotShift = style({ marginTop: "6px" });

/**
 * ★★ A navigable note reads as navigable, and it is the SAME hover the rest of
 * the cockpit uses. A second, prettier affordance for one row type would teach
 * a person that this app has more than one kind of clickable thing, which is
 * the maze it is trying not to be.
 */
/** ★★ A meta label that is also a door. Same hover as every other one. */
export const metaButton = style({
  fontFamily: vars.font.mono,
  fontSize: "10.5px",
  color: vars.color.textSecondary,
  background: "transparent",
  border: "none",
  padding: 0,
  cursor: "pointer",
  selectors: {
    "&:hover": { color: vars.color.amber },
    "&:focus-visible": { outline: `1px solid ${vars.color.amber}`, outlineOffset: "2px" },
  },
});

export const noteRowButton = style({
  width: "100%",
  textAlign: "left",
  background: "transparent",
  border: "none",
  color: "inherit",
  font: "inherit",
  cursor: "pointer",
  selectors: {
    "&:hover": { background: vars.color.bgRaised },
    "&:focus-visible": { outline: `1px solid ${vars.color.amber}`, outlineOffset: "-1px" },
  },
});

/* ── meter ──────────────────────────────────────────────────────────────── */

export const meterTrack = style({
  position: "relative",
  height: "4px",
  borderRadius: "2px",
  background: vars.color.bgOverlay,
  overflow: "hidden",
  marginTop: vars.space.xs,
});

export const meterFill = style({
  position: "absolute",
  inset: 0,
  right: "auto",
  borderRadius: "2px",
  transition: "width 180ms ease-out",
});

/* ── telemetry ──────────────────────────────────────────────────────────── */

export const telemetryStrip = style({
  display: "grid",
  gridTemplateColumns: "repeat(auto-fit, minmax(92px, 1fr))",
  gap: vars.space.md,
  padding: `${vars.space.md} 0`,
  borderTop: `1px solid ${vars.color.border}`,
  minWidth: 0,
});

export const telemetryCell = style({ display: "flex", flexDirection: "column", gap: "2px", minWidth: 0 });

/* ── controls ───────────────────────────────────────────────────────────── */

export const field = style({ display: "flex", flexDirection: "column", gap: vars.space.xs, minWidth: 0 });

export const input = style({
  fontFamily: vars.font.mono,
  fontSize: "12px",
  color: vars.color.textPrimary,
  background: vars.color.bgBase,
  border: `1px solid ${vars.color.borderMid}`,
  borderRadius: vars.radius.sm,
  padding: `${vars.space.sm} ${vars.space.md}`,
  outline: "none",
  minWidth: 0,
  width: "100%",
  selectors: { "&:focus": { borderColor: vars.color.amberBorder } },
});

export const select = style([input, { padding: `6px ${vars.space.sm}`, cursor: "pointer" }]);

const buttonBase = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.body,
  fontWeight: 500,
  letterSpacing: "0.08em",
  textTransform: "uppercase",
  borderRadius: vars.radius.sm,
  padding: `${vars.space.sm} ${vars.space.lg}`,
  cursor: "pointer",
  whiteSpace: "nowrap",
  selectors: { "&:disabled": { opacity: 0.45, cursor: "default" } },
});

export const button = styleVariants({
  primary: [
    buttonBase,
    {
      color: vars.color.bgBase,
      background: vars.color.amber,
      border: "1px solid transparent",
      selectors: { "&:hover:not(:disabled)": { background: vars.color.amberDim } },
    },
  ],
  ghost: [
    buttonBase,
    {
      color: vars.color.textSecondary,
      background: "transparent",
      border: `1px solid ${vars.color.borderMid}`,
      selectors: {
        "&:hover:not(:disabled)": { background: vars.color.bgRaised, color: vars.color.textPrimary },
      },
    },
  ],
});

/** A small selectable token — presets, operator names, actions in a row. */
export const chip = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.label,
  letterSpacing: "0.06em",
  color: vars.color.textSecondary,
  background: vars.color.bgRaised,
  border: `1px solid ${vars.color.border}`,
  borderRadius: vars.radius.sm,
  padding: `${vars.space.xs} ${vars.space.sm}`,
  cursor: "pointer",
  whiteSpace: "nowrap",
  selectors: {
    "&:hover:not(:disabled)": { borderColor: vars.color.amberBorder, color: vars.color.textPrimary },
    "&:disabled": { opacity: 0.45, cursor: "default" },
  },
});

/** ★ A chip that is the CURRENT choice. The cockpit's only "tab" mechanism. */
export const chipActive = style([
  chip,
  { color: vars.color.amber, borderColor: vars.color.amberBorder, background: vars.color.amberGlow },
]);

export const codeBlock = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.meta,
  lineHeight: 1.5,
  color: vars.color.textSecondary,
  background: vars.color.bgBase,
  border: `1px solid ${vars.color.border}`,
  borderRadius: vars.radius.sm,
  padding: vars.space.md,
  margin: 0,
  maxHeight: "260px",
  overflow: "auto",
  whiteSpace: "pre-wrap",
  overflowWrap: "anywhere",
});

/* ── shell identity ─────────────────────────────────────────────────────── */

export const brand = style({
  fontFamily: vars.font.mono,
  fontSize: "12px",
  fontWeight: 500,
  letterSpacing: "0.16em",
  color: vars.color.amber,
  whiteSpace: "nowrap",
});

export const avatar = style({
  width: "20px",
  height: "20px",
  borderRadius: vars.radius.sm,
  background: vars.color.amberGlow,
  border: `1px solid ${vars.color.amberBorder}`,
  color: vars.color.amber,
  fontFamily: vars.font.mono,
  fontSize: vars.size.micro,
  fontWeight: 500,
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  flexShrink: 0,
});

export const avatarLarge = style([
  avatar,
  { width: "44px", height: "44px", borderRadius: vars.radius.md, fontSize: "16px" },
]);

/* ── the lock screen ────────────────────────────────────── */

/** ★ The whole viewport, because there is nothing behind it to see. */
export const lockFrame = style({
  display: "grid",
  placeItems: "center",
  minHeight: "100%",
  padding: vars.space.lg,
  minWidth: 0,
});

export const lockPanel = style({
  width: "100%",
  maxWidth: "420px",
  display: "flex",
  flexDirection: "column",
  gap: vars.space.sm,
  minWidth: 0,
});

/* ── constellation ──────────────────────────────────────────────────────── */

/**
 * ★★★ **`height: auto` was the clip.** With a fixed aspect ratio, `auto`
 * height means the drawing is only ever sized by the WIDTH available — so on a
 * short pane it paints taller than the pane and the bottom of the graph goes
 * under whatever is below. Constraining both axes and letting
 * `preserveAspectRatio` scale into whatever box it gets makes fitting a
 * property of the element rather than of the window.
 */
export const constellationSvg = style({
  width: "100%",
  height: "100%",
  maxWidth: "100%",
  maxHeight: "100%",
  minHeight: 0,
  display: "block",
});
export const node = style({ cursor: "pointer" });
globalStyle(`${node}:hover circle`, { stroke: vars.color.amber });
export const nodeLabel = style({ fontFamily: vars.font.ui, fontSize: "11px", fontWeight: 500 });
export const nodeFigure = style({ fontFamily: vars.font.mono, fontSize: "9.5px" });

/* ── stream ─────────────────────────────────────────────────────────────── */

export const streamRow = style({
  display: "grid",
  gridTemplateColumns: "76px 1fr",
  gap: vars.space.sm,
  alignItems: "baseline",
  padding: `${vars.space.sm} 0`,
  borderTop: `1px solid ${vars.color.border}`,
  fontFamily: vars.font.mono,
  fontSize: vars.size.body,
  minWidth: 0,
});

export const streamRowRefused = style([
  streamRow,
  { background: vars.color.dangerGlow, borderTopColor: vars.color.dangerBorder },
]);

export const streamVerdict = style({
  fontFamily: vars.font.mono,
  fontSize: "9.5px",
  fontWeight: 500,
  letterSpacing: "0.10em",
});

/* ── grids used inside cards ────────────────────────────────────────────── */

/** id · value · action — collapses to a single column on a phone. */
export const controlRow = style({
  display: "grid",
  gridTemplateColumns: "1fr auto auto",
  gap: vars.space.sm,
  alignItems: "center",
  padding: `${vars.space.sm} 0`,
  borderTop: `1px solid ${vars.color.border}`,
  minWidth: 0,
  "@media": { [bp.sm]: { gridTemplateColumns: "1fr" } },
});

export const logRow = style({
  display: "grid",
  gridTemplateColumns: "auto 1fr auto",
  gap: vars.space.sm,
  alignItems: "baseline",
  padding: `${vars.space.xs} 0`,
  borderTop: `1px solid ${vars.color.border}`,
  fontFamily: vars.font.mono,
  fontSize: vars.size.meta,
  color: vars.color.textSecondary,
  minWidth: 0,
});

export const seqCell = style({ color: vars.color.textDim, fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap" });

export const diffRow = style({
  display: "grid",
  gridTemplateColumns: "1fr auto auto",
  gap: vars.space.sm,
  alignItems: "baseline",
  padding: `${vars.space.xs} 0`,
  borderTop: `1px solid ${vars.color.border}`,
  minWidth: 0,
});

/** The household selector rows. */
export const sustainRow = style({
  display: "grid",
  gridTemplateColumns: "auto 1fr auto",
  alignItems: "baseline",
  gap: vars.space.sm,
  padding: vars.space.sm,
  border: "1px solid transparent",
  borderRadius: vars.radius.sm,
  cursor: "pointer",
  textAlign: "left",
  background: "transparent",
  color: "inherit",
  font: "inherit",
  width: "100%",
  minWidth: 0,
  selectors: { "&:hover": { background: vars.color.bgRaised } },
});

export const sustainRowActive = style([
  sustainRow,
  { borderColor: vars.color.amberBorder, background: vars.color.amberGlow },
]);

export const childIndent = style({ paddingLeft: vars.space.xl });
export const sustainName = style({ fontFamily: vars.font.ui, fontSize: "13px", fontWeight: 500, color: vars.color.textPrimary });
export const pawaMeasured = style({ fontFamily: vars.font.mono, fontSize: vars.size.meta, color: vars.color.teal, fontVariantNumeric: "tabular-nums", textAlign: "right" });
export const pawaUnmeasured = style({ fontFamily: vars.font.mono, fontSize: vars.size.meta, color: vars.color.textDim, textAlign: "right" });

/** An operator in the Console picker. */
export const opRow = style({
  display: "grid",
  gridTemplateColumns: "1fr auto",
  gap: vars.space.sm,
  alignItems: "baseline",
  padding: vars.space.sm,
  borderTop: `1px solid ${vars.color.border}`,
  background: "transparent",
  border: "none",
  borderTopWidth: "1px",
  borderTopStyle: "solid",
  borderTopColor: vars.color.border,
  color: "inherit",
  font: "inherit",
  textAlign: "left",
  width: "100%",
  minWidth: 0,
  cursor: "pointer",
  selectors: { "&:hover": { background: vars.color.bgRaised } },
});

export const opRowActive = style([opRow, { background: vars.color.amberGlow }]);

export const pocketBlock = style({ padding: `${vars.space.sm} 0`, borderTop: `1px solid ${vars.color.border}`, minWidth: 0 });
export const pocketHead = style({ display: "grid", gridTemplateColumns: "1fr auto auto", gap: vars.space.md, alignItems: "baseline", minWidth: 0 });
