/**
 * Layout primitives — where the cockpit's responsive behaviour is decided.
 *
 * ★★★ Responsive behaviour lives in `src/ui`, never in a screen. **No screen
 * file contains a media query** — arrangement is decided here, and the three
 * density adjustments an instrument makes about itself (a big figure, a card's
 * padding, a three-column control row) live beside those instruments in
 * `ui.css.ts`. A screen composes primitives and inherits both. A screen composes
 * `Split`/`Stack`/`Cluster` and inherits narrow-width behaviour it never had to
 * think about; nothing below has a media query of its own. That is what makes
 * "no horizontal scroll at 375px" a property of the system rather than a thing
 * twelve screens each remember.
 *
 * ★★ It is also the groundwork for **Orchie**. A phone-first face on this same
 * codebase needs primitives that already collapse correctly, not a second set
 * of screens — so these are built to be re-composed at a different density
 * rather than re-written.
 *
 * ★ Every primitive sets `min-width: 0`. Without it a grid or flex child
 * refuses to shrink below its content and pushes the page sideways — which is
 * the single most common cause of the horizontal scroll this file exists to
 * prevent.
 */
import { style, styleVariants } from "@vanilla-extract/css";
import { bp, vars } from "./tokens.css";

/* ── the app frame ──────────────────────────────────────────────────────── */

export const frame = style({
  display: "grid",
  gridTemplateRows: "auto 1fr auto",
  height: "100%",
  minWidth: 0,
  maxWidth: "100vw",
});

export const body = style({
  display: "grid",
  gridTemplateColumns: "212px 1fr",
  minHeight: 0,
  minWidth: 0,
  "@media": {
    // The rail becomes a horizontally-scrollable strip above the panel.
    [bp.md]: { gridTemplateColumns: "1fr", gridTemplateRows: "auto 1fr" },
  },
});

/** The screen slot. Everything a panel renders lives inside this. */
export const panelSlot = style({
  minWidth: 0,
  minHeight: 0,
  overflow: "hidden",
  display: "grid",
});

/* ── Split — the cockpit's two-column panel ─────────────────────────────── */

/**
 * ★★ The workhorse. Two scrollable columns on a wide window, one stacked
 * column below `md`. Every panel uses it, which is why they all behave the same
 * way when the window narrows.
 */
export const split = styleVariants({
  even: [
    {
      display: "grid",
      gridTemplateColumns: "1fr 1fr",
      gap: vars.space.lg,
      padding: vars.space.lg,
      minHeight: 0,
      minWidth: 0,
      overflow: "hidden",
      "@media": {
        [bp.md]: { gridTemplateColumns: "1fr", overflowY: "auto", gap: vars.space.md },
        [bp.sm]: { padding: vars.space.sm },
      },
    },
  ],
  /** A wide left (the graph) and a narrower right (the stream). */
  wideLeft: [
    {
      display: "grid",
      gridTemplateColumns: "minmax(420px, 1fr) minmax(320px, 420px)",
      gap: vars.space.lg,
      padding: vars.space.lg,
      minHeight: 0,
      minWidth: 0,
      overflow: "hidden",
      "@media": {
        [bp.md]: { gridTemplateColumns: "1fr", overflowY: "auto", gap: vars.space.md },
        [bp.sm]: { padding: vars.space.sm },
      },
    },
  ],
  /** A narrower left (controls) and a wide right (results). */
  wideRight: [
    {
      display: "grid",
      gridTemplateColumns: "minmax(300px, 400px) 1fr",
      gap: vars.space.lg,
      padding: vars.space.lg,
      minHeight: 0,
      minWidth: 0,
      overflow: "hidden",
      "@media": {
        [bp.md]: { gridTemplateColumns: "1fr", overflowY: "auto", gap: vars.space.md },
        [bp.sm]: { padding: vars.space.sm },
      },
    },
  ],
});

/** A column inside a `Split`. Scrolls on its own when there is room to. */
export const column = style({
  display: "flex",
  flexDirection: "column",
  gap: vars.space.lg,
  minHeight: 0,
  minWidth: 0,
  overflowY: "auto",
  "@media": {
    // Below `md` the SPLIT scrolls, not each column — two independent
    // scrollers stacked on a phone is a trap.
    [bp.md]: { overflowY: "visible", gap: vars.space.md },
  },
});

/* ── Stack / Cluster / Cols ─────────────────────────────────────────────── */

export const stack = styleVariants({
  xs: [{ display: "flex", flexDirection: "column", gap: vars.space.xs, minWidth: 0 }],
  sm: [{ display: "flex", flexDirection: "column", gap: vars.space.sm, minWidth: 0 }],
  md: [{ display: "flex", flexDirection: "column", gap: vars.space.md, minWidth: 0 }],
  lg: [{ display: "flex", flexDirection: "column", gap: vars.space.lg, minWidth: 0 }],
});

/** ★ Horizontal, and it **wraps**. A cluster that could not wrap would be the
 *  thing that pushes a narrow window sideways. */
export const cluster = styleVariants({
  sm: [{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: vars.space.sm, minWidth: 0 }],
  md: [{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: vars.space.md, minWidth: 0 }],
});

/** An auto-fitting grid. Never fewer columns than fit, never overflowing. */
export const cols = style({
  display: "grid",
  gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))",
  gap: vars.space.md,
  minWidth: 0,
});

/* ── the shell chrome ───────────────────────────────────────────────────── */

export const topbar = style({
  display: "flex",
  alignItems: "center",
  gap: vars.space.md,
  padding: `0 ${vars.space.lg}`,
  minHeight: "44px",
  borderBottom: `1px solid ${vars.color.border}`,
  background: vars.color.bgSurface,
  minWidth: 0,
  "@media": {
    [bp.md]: { flexWrap: "wrap", padding: `${vars.space.sm} ${vars.space.md}`, gap: vars.space.sm },
  },
});

export const nav = style({
  display: "flex",
  flexDirection: "column",
  gap: vars.space.md,
  padding: vars.space.md,
  borderRight: `1px solid ${vars.color.border}`,
  background: vars.color.bgSurface,
  overflowY: "auto",
  minHeight: 0,
  minWidth: 0,
  "@media": {
    // ★ A horizontal rail on a narrow window. It scrolls sideways ITSELF, which
    //   is the one place a sideways scroll is correct — the page still does not.
    [bp.md]: {
      flexDirection: "row",
      overflowX: "auto",
      overflowY: "hidden",
      borderRight: "none",
      borderBottom: `1px solid ${vars.color.border}`,
      padding: vars.space.sm,
      gap: vars.space.sm,
    },
  },
});

export const navGroup = style({
  display: "flex",
  flexDirection: "column",
  gap: "1px",
  minWidth: 0,
  "@media": { [bp.md]: { flexDirection: "row", gap: vars.space.xs } },
});

export const navGroupTitle = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.micro,
  fontWeight: 500,
  letterSpacing: "0.14em",
  textTransform: "uppercase",
  color: vars.color.textDim,
  padding: `${vars.space.xs} ${vars.space.sm}`,
  whiteSpace: "nowrap",
  "@media": { [bp.md]: { display: "none" } },
});

const navItemBase = style({
  display: "flex",
  alignItems: "center",
  gap: vars.space.sm,
  fontFamily: vars.font.ui,
  fontSize: "12.5px",
  background: "transparent",
  border: "1px solid transparent",
  borderRadius: vars.radius.sm,
  padding: `${vars.space.sm} ${vars.space.sm}`,
  cursor: "pointer",
  textAlign: "left",
  width: "100%",
  whiteSpace: "nowrap",
  "@media": { [bp.md]: { width: "auto" } },
});

export const navItem = styleVariants({
  idle: [navItemBase, {
    color: vars.color.textSecondary,
    selectors: { "&:hover": { background: vars.color.bgRaised, color: vars.color.textPrimary } },
  }],
  active: [navItemBase, {
    color: vars.color.amber,
    borderColor: vars.color.amberBorder,
    background: vars.color.amberGlow,
  }],
});

export const navCount = style({
  fontFamily: vars.font.mono,
  fontSize: vars.size.micro,
  letterSpacing: "0.08em",
  color: vars.color.textDim,
  marginLeft: "auto",
  "@media": { [bp.md]: { marginLeft: vars.space.xs } },
});

/** The rail's footer note. Hidden on the horizontal rail, where there is no room. */
export const navFooter = style({
  marginTop: "auto",
  minWidth: 0,
  "@media": { [bp.md]: { display: "none" } },
});

export const statusBelt = style({
  display: "flex",
  alignItems: "center",
  gap: vars.space.md,
  padding: `${vars.space.xs} ${vars.space.lg}`,
  borderTop: `1px solid ${vars.color.border}`,
  background: vars.color.bgSurface,
  fontFamily: vars.font.mono,
  fontSize: vars.size.label,
  letterSpacing: "0.06em",
  color: vars.color.textMuted,
  overflowX: "auto",
  whiteSpace: "nowrap",
  minWidth: 0,
  "@media": { [bp.sm]: { padding: `${vars.space.xs} ${vars.space.sm}` } },
});

export const beltCell = style({ display: "flex", alignItems: "baseline", gap: vars.space.xs, flexShrink: 0 });

/** ★ Chrome that is real but not essential, hidden when the window is narrow. */
/** The panel slot's own padding, for the shell-level loading and error states. */
export const panelPad = style({ padding: vars.space.xl, minWidth: 0 });

/** A nav count that is reporting something real and non-zero. */
export const navCountLive = style([navCount, { color: vars.color.warn }]);

/** The reading half of a status-belt cell. */
export const beltValue = style({ color: vars.color.textSecondary });

/** A belt cell that cannot be honest, so it is a dash and says why. */
export const beltAbsent = style({ color: vars.color.textDim });

export const hideNarrow = style({ "@media": { [bp.sm]: { display: "none" } } });
