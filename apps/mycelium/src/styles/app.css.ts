import { globalStyle, style, keyframes } from "@vanilla-extract/css";
import { vars } from "./tokens.css";

globalStyle("*, *::before, *::after", { boxSizing: "border-box" });
globalStyle("html, body, #root", { height: "100%", margin: 0 });
globalStyle("body", {
  background: vars.color.bgBase,
  color: vars.color.textPrimary,
  fontFamily: vars.font.ui,
  fontSize: "13px",
  WebkitFontSmoothing: "antialiased",
});
globalStyle("::-webkit-scrollbar", { width: "6px", height: "6px" });
globalStyle("::-webkit-scrollbar-track", { background: "transparent" });
globalStyle("::-webkit-scrollbar-thumb", {
  background: vars.color.borderMid,
  borderRadius: "3px",
});

/* ── shell ─────────────────────────────────────────────────────────────── */

export const shell = style({
  display: "grid",
  gridTemplateRows: "auto 1fr auto",
  height: "100%",
  minWidth: 0,
});

export const topbar = style({
  display: "flex",
  alignItems: "center",
  gap: vars.space.lg,
  padding: `0 ${vars.space.lg}`,
  height: "44px",
  borderBottom: `1px solid ${vars.color.border}`,
  background: vars.color.bgSurface,
});

export const brand = style({
  fontFamily: vars.font.mono,
  fontSize: "12px",
  fontWeight: 500,
  letterSpacing: "0.16em",
  textTransform: "uppercase",
  color: vars.color.amber,
});

export const brandSub = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  letterSpacing: "0.10em",
  textTransform: "uppercase",
  color: vars.color.textDim,
});

export const spacer = style({ flex: 1 });

export const main = style({
  display: "grid",
  gridTemplateColumns: "minmax(340px, 420px) 1fr",
  gap: vars.space.lg,
  padding: vars.space.lg,
  minHeight: 0,
  minWidth: 0,
  overflow: "hidden",
  "@media": { "screen and (max-width: 900px)": { gridTemplateColumns: "1fr" } },
});

export const column = style({
  display: "flex",
  flexDirection: "column",
  gap: vars.space.lg,
  minHeight: 0,
  minWidth: 0,
  overflowY: "auto",
});

/* ── card ──────────────────────────────────────────────────────────────── */

export const card = style({
  border: `1px solid ${vars.color.border}`,
  background: vars.color.bgSurface,
  borderRadius: vars.radius.md,
  minWidth: 0,
});

export const cardHead = style({
  display: "flex",
  alignItems: "center",
  gap: vars.space.sm,
  padding: `${vars.space.sm} ${vars.space.md}`,
  borderBottom: `1px solid ${vars.color.border}`,
});

export const cardTitle = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  fontWeight: 500,
  letterSpacing: "0.10em",
  textTransform: "uppercase",
  color: vars.color.textMuted,
});

export const cardBody = style({ padding: vars.space.md });

/* ── typography ────────────────────────────────────────────────────────── */

export const label = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  fontWeight: 500,
  letterSpacing: "0.10em",
  textTransform: "uppercase",
  color: vars.color.textMuted,
});

export const mono = style({
  fontFamily: vars.font.mono,
  fontSize: "11px",
  color: vars.color.textSecondary,
});

export const value = style({
  fontFamily: vars.font.mono,
  fontSize: "13px",
  fontWeight: 500,
  color: vars.color.textPrimary,
  fontVariantNumeric: "tabular-nums",
});

export const valueBig = style({
  fontFamily: vars.font.mono,
  fontSize: "26px",
  fontWeight: 500,
  color: vars.color.textPrimary,
  letterSpacing: "-0.01em",
  fontVariantNumeric: "tabular-nums",
});

export const empty = style({
  fontFamily: vars.font.mono,
  fontSize: "11px",
  color: vars.color.textMuted,
  padding: `${vars.space.md} 0`,
});

/* ── rows ──────────────────────────────────────────────────────────────── */

export const row = style({
  display: "flex",
  alignItems: "baseline",
  justifyContent: "space-between",
  gap: vars.space.md,
  padding: `${vars.space.xs} 0`,
});

export const pocketRow = style({
  display: "grid",
  gridTemplateColumns: "1fr auto auto auto",
  gap: vars.space.md,
  alignItems: "baseline",
  padding: `${vars.space.sm} 0`,
  borderTop: `1px solid ${vars.color.border}`,
});

export const invariantRow = style({
  display: "flex",
  gap: vars.space.sm,
  alignItems: "baseline",
  padding: `${vars.space.xs} 0`,
  fontFamily: vars.font.mono,
  fontSize: "11px",
  color: vars.color.textSecondary,
  wordBreak: "break-word",
});

/* ── controls ──────────────────────────────────────────────────────────── */

export const field = style({
  display: "flex",
  flexDirection: "column",
  gap: vars.space.xs,
  minWidth: 0,
});

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
  selectors: {
    "&:focus": { borderColor: vars.color.amberBorder },
  },
});

export const button = style({
  fontFamily: vars.font.mono,
  fontSize: "11px",
  fontWeight: 500,
  letterSpacing: "0.08em",
  textTransform: "uppercase",
  color: vars.color.bgBase,
  background: vars.color.amber,
  border: "1px solid transparent",
  borderRadius: vars.radius.sm,
  padding: `${vars.space.sm} ${vars.space.lg}`,
  cursor: "pointer",
  selectors: {
    "&:hover": { background: vars.color.amberDim },
    "&:disabled": { opacity: 0.45, cursor: "default" },
  },
});

export const buttonGhost = style([
  button,
  {
    color: vars.color.textSecondary,
    background: "transparent",
    border: `1px solid ${vars.color.borderMid}`,
    selectors: {
      "&:hover": { background: vars.color.bgRaised, color: vars.color.textPrimary },
    },
  },
]);

export const presetRow = style({
  display: "flex",
  flexWrap: "wrap",
  gap: vars.space.sm,
});

export const chip = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  letterSpacing: "0.06em",
  color: vars.color.textSecondary,
  background: vars.color.bgRaised,
  border: `1px solid ${vars.color.border}`,
  borderRadius: vars.radius.sm,
  padding: `${vars.space.xs} ${vars.space.sm}`,
  cursor: "pointer",
  selectors: {
    "&:hover": { borderColor: vars.color.amberBorder, color: vars.color.textPrimary },
  },
});

/* ── the verdict — the screen this skeleton exists to prove ────────────── */

const flash = keyframes({
  "0%": { opacity: 0.35 },
  "100%": { opacity: 1 },
});

export const verdictCard = style({
  border: `1px solid ${vars.color.border}`,
  borderRadius: vars.radius.md,
  padding: vars.space.md,
  animation: `${flash} 220ms ease-out`,
});

export const verdictAdmitted = style([
  verdictCard,
  { borderColor: vars.color.tealBorder, background: vars.color.tealGlow },
]);

export const verdictRefused = style([
  verdictCard,
  { borderColor: vars.color.dangerBorder, background: vars.color.dangerGlow },
]);

export const verdictDeferred = style([
  verdictCard,
  { borderColor: vars.color.amberBorder, background: vars.color.amberGlow },
]);

export const verdictBadge = style({
  fontFamily: vars.font.mono,
  fontSize: "12px",
  fontWeight: 500,
  letterSpacing: "0.16em",
  textTransform: "uppercase",
});

export const badgeAdmitted = style([verdictBadge, { color: vars.color.teal }]);
export const badgeRefused = style([verdictBadge, { color: vars.color.danger }]);
export const badgeDeferred = style([verdictBadge, { color: vars.color.amber }]);

export const reason = style({
  fontFamily: vars.font.mono,
  fontSize: "11.5px",
  lineHeight: 1.55,
  color: vars.color.textPrimary,
  marginTop: vars.space.sm,
  wordBreak: "break-word",
});

export const reasonCode = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  letterSpacing: "0.08em",
  textTransform: "uppercase",
  color: vars.color.textMuted,
  marginTop: vars.space.xs,
});

export const dot = style({
  width: "6px",
  height: "6px",
  borderRadius: "50%",
  display: "inline-block",
});

export const codeBlock = style({
  fontFamily: vars.font.mono,
  fontSize: "10.5px",
  lineHeight: 1.5,
  color: vars.color.textSecondary,
  background: vars.color.bgBase,
  border: `1px solid ${vars.color.border}`,
  borderRadius: vars.radius.sm,
  padding: vars.space.md,
  margin: 0,
  maxHeight: "260px",
  overflow: "auto",
  whiteSpace: "pre",
});

export const errorBox = style({
  border: `1px solid ${vars.color.dangerBorder}`,
  background: vars.color.dangerGlow,
  borderRadius: vars.radius.md,
  padding: vars.space.md,
  fontFamily: vars.font.mono,
  fontSize: "11px",
  color: vars.color.textPrimary,
});

/* ── the household selector ────────────────────────────────────────────── */

export const selector = style({
  display: "flex",
  flexDirection: "column",
  gap: "2px",
});

export const sustainRow = style({
  display: "grid",
  gridTemplateColumns: "auto 1fr auto",
  alignItems: "baseline",
  gap: vars.space.sm,
  padding: `${vars.space.sm} ${vars.space.sm}`,
  border: "1px solid transparent",
  borderRadius: vars.radius.sm,
  cursor: "pointer",
  textAlign: "left",
  background: "transparent",
  color: "inherit",
  font: "inherit",
  width: "100%",
  selectors: {
    "&:hover": { background: vars.color.bgRaised },
  },
});

export const sustainRowActive = style([
  sustainRow,
  {
    borderColor: vars.color.amberBorder,
    background: vars.color.amberGlow,
  },
]);

/** A child is indented to show `⊕` without drawing a tree nobody asked for. */
export const childIndent = style({ paddingLeft: vars.space.xl });

export const sustainName = style({
  fontFamily: vars.font.ui,
  fontSize: "13px",
  fontWeight: 500,
  color: vars.color.textPrimary,
});

export const sustainMeta = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  letterSpacing: "0.06em",
  color: vars.color.textMuted,
});

export const sustainFigure = style({
  fontFamily: vars.font.mono,
  fontSize: "12px",
  color: vars.color.textSecondary,
  fontVariantNumeric: "tabular-nums",
});

/* ── honest unavailable ────────────────────────────────────────────────── */

export const unavailable = style({
  border: `1px dashed ${vars.color.borderMid}`,
  borderRadius: vars.radius.md,
  padding: vars.space.md,
  fontFamily: vars.font.mono,
  fontSize: "11px",
  lineHeight: 1.55,
  color: vars.color.textMuted,
});

export const unavailableTitle = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  fontWeight: 500,
  letterSpacing: "0.10em",
  textTransform: "uppercase",
  color: vars.color.textSecondary,
  display: "block",
  marginBottom: vars.space.xs,
});

export const statusBelt = style({
  display: "flex",
  alignItems: "center",
  gap: vars.space.md,
  padding: `${vars.space.xs} ${vars.space.lg}`,
  borderTop: `1px solid ${vars.color.border}`,
  background: vars.color.bgSurface,
  fontFamily: vars.font.mono,
  fontSize: "10px",
  letterSpacing: "0.06em",
  color: vars.color.textMuted,
  overflowX: "auto",
  whiteSpace: "nowrap",
});

/* ── monitor ───────────────────────────────────────────────────────────── */

export const tabs = style({ display: "flex", gap: "2px" });

export const tab = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  fontWeight: 500,
  letterSpacing: "0.10em",
  textTransform: "uppercase",
  color: vars.color.textMuted,
  background: "transparent",
  border: "1px solid transparent",
  borderRadius: vars.radius.sm,
  padding: `${vars.space.xs} ${vars.space.md}`,
  cursor: "pointer",
  selectors: { "&:hover": { color: vars.color.textPrimary } },
});

export const tabActive = style([
  tab,
  { color: vars.color.amber, borderColor: vars.color.amberBorder, background: vars.color.amberGlow },
]);

/** A pocket's fill, drawn from its own real numbers. */
export const meter = style({
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

export const pocketBlock = style({
  padding: `${vars.space.sm} 0`,
  borderTop: `1px solid ${vars.color.border}`,
});

export const pocketHead = style({
  display: "grid",
  gridTemplateColumns: "1fr auto auto",
  gap: vars.space.md,
  alignItems: "baseline",
});

export const logRow = style({
  display: "grid",
  gridTemplateColumns: "34px 1fr auto",
  gap: vars.space.sm,
  alignItems: "baseline",
  padding: `${vars.space.xs} 0`,
  borderTop: `1px solid ${vars.color.border}`,
  fontFamily: vars.font.mono,
  fontSize: "10.5px",
  color: vars.color.textSecondary,
});

export const seqCell = style({
  color: vars.color.textDim,
  fontVariantNumeric: "tabular-nums",
});

export const attentionRow = style({
  display: "flex",
  gap: vars.space.sm,
  alignItems: "baseline",
  padding: `${vars.space.sm} 0`,
  borderTop: `1px solid ${vars.color.border}`,
});

export const attentionWhy = style({
  fontFamily: vars.font.mono,
  fontSize: "10.5px",
  color: vars.color.textMuted,
});

export const pushBadge = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  letterSpacing: "0.08em",
  color: vars.color.teal,
  border: `1px solid ${vars.color.tealBorder}`,
  background: vars.color.tealGlow,
  borderRadius: vars.radius.sm,
  padding: `2px ${vars.space.sm}`,
});

/* -- the app shell (V1.3) ------------------------------------------------ */

export const frame = style({
  display: "grid",
  gridTemplateRows: "44px 1fr auto",
  height: "100%",
  minWidth: 0,
});

export const body = style({
  display: "grid",
  gridTemplateColumns: "212px 1fr",
  minHeight: 0,
  minWidth: 0,
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
});

export const navGroup = style({
  display: "flex",
  flexDirection: "column",
  gap: "1px",
});

export const navGroupTitle = style({
  fontFamily: vars.font.mono,
  fontSize: "9px",
  fontWeight: 500,
  letterSpacing: "0.14em",
  textTransform: "uppercase",
  color: vars.color.textDim,
  padding: `${vars.space.xs} ${vars.space.sm}`,
});

export const navItem = style({
  display: "flex",
  alignItems: "center",
  gap: vars.space.sm,
  fontFamily: vars.font.ui,
  fontSize: "12.5px",
  color: vars.color.textSecondary,
  background: "transparent",
  border: "1px solid transparent",
  borderRadius: vars.radius.sm,
  padding: `${vars.space.sm} ${vars.space.sm}`,
  cursor: "pointer",
  textAlign: "left",
  width: "100%",
  selectors: {
    "&:hover:not(:disabled)": { background: vars.color.bgRaised, color: vars.color.textPrimary },
    "&:disabled": { color: vars.color.textDim, cursor: "default" },
  },
});

export const navItemActive = style([
  navItem,
  {
    color: vars.color.amber,
    borderColor: vars.color.amberBorder,
    background: vars.color.amberGlow,
  },
]);

/** A panel that exists in the roadmap but not yet in the app. */
export const navSoon = style({
  fontFamily: vars.font.mono,
  fontSize: "9px",
  letterSpacing: "0.08em",
  color: vars.color.textDim,
  marginLeft: "auto",
});

export const identity = style({
  fontFamily: vars.font.mono,
  fontSize: "11px",
  color: vars.color.textSecondary,
  letterSpacing: "0.04em",
});

export const avatar = style({
  width: "20px",
  height: "20px",
  borderRadius: "2px",
  background: vars.color.amberGlow,
  border: `1px solid ${vars.color.amberBorder}`,
  color: vars.color.amber,
  fontFamily: vars.font.mono,
  fontSize: "9px",
  fontWeight: 500,
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
});

export const picker = style({
  fontFamily: vars.font.mono,
  fontSize: "11px",
  color: vars.color.textPrimary,
  background: vars.color.bgRaised,
  border: `1px solid ${vars.color.borderMid}`,
  borderRadius: vars.radius.sm,
  padding: `3px ${vars.space.sm}`,
  outline: "none",
  maxWidth: "220px",
});

export const beltCell = style({
  display: "flex",
  alignItems: "baseline",
  gap: vars.space.xs,
});

export const beltValue = style({ color: vars.color.textSecondary });
export const beltAbsent = style({ color: vars.color.textDim });

/* -- constellation ------------------------------------------------------- */

export const constellationGrid = style({
  display: "grid",
  gridTemplateColumns: "minmax(420px, 1fr) minmax(320px, 420px)",
  gap: vars.space.lg,
  padding: vars.space.lg,
  minHeight: 0,
  minWidth: 0,
  overflow: "hidden",
  "@media": { "screen and (max-width: 1040px)": { gridTemplateColumns: "1fr" } },
});

export const constellationSvg = style({
  width: "100%",
  height: "auto",
  display: "block",
});

export const node = style({ cursor: "pointer" });

// vanilla-extract refuses a descendant selector inside `style` (it would leak
// specificity into elements it does not own). A hover on a child element is a
// global rule scoped to this class, and saying so is the fix.
globalStyle(`${node}:hover circle`, { stroke: vars.color.amber });

export const nodeLabel = style({
  fontFamily: vars.font.ui,
  fontSize: "11px",
  fontWeight: 500,
});

export const nodeFigure = style({
  fontFamily: vars.font.mono,
  fontSize: "9.5px",
});

export const telemetry = style({
  display: "grid",
  gridTemplateColumns: "repeat(auto-fit, minmax(96px, 1fr))",
  gap: vars.space.md,
  padding: `${vars.space.md} 0`,
  marginTop: vars.space.md,
  borderTop: `1px solid ${vars.color.border}`,
});

export const telemetryCell = style({
  display: "flex",
  flexDirection: "column",
  gap: "2px",
});

export const streamRow = style({
  display: "grid",
  gridTemplateColumns: "76px 1fr",
  gap: vars.space.sm,
  alignItems: "baseline",
  padding: `${vars.space.sm} 0`,
  borderTop: `1px solid ${vars.color.border}`,
  fontFamily: vars.font.mono,
  fontSize: "11px",
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

export const refusedBadge = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  letterSpacing: "0.08em",
  color: vars.color.danger,
  border: `1px solid ${vars.color.dangerBorder}`,
  background: vars.color.dangerGlow,
  borderRadius: vars.radius.sm,
  padding: `2px ${vars.space.sm}`,
  marginLeft: vars.space.sm,
});

export const attentionButton = style({
  display: "flex",
  gap: vars.space.sm,
  alignItems: "baseline",
  padding: `${vars.space.sm} 0`,
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
  cursor: "pointer",
  selectors: { "&:hover": { background: vars.color.bgRaised } },
});

/* -- V1.4 ---------------------------------------------------------------- */

/** A hypothetical is never allowed to look like a fact. */
export const hypothetical = style({
  border: `1px dashed ${vars.color.amberBorder}`,
  background: vars.color.amberGlow,
  borderRadius: vars.radius.md,
  padding: vars.space.md,
  fontFamily: vars.font.mono,
  fontSize: "10.5px",
  lineHeight: 1.55,
  color: vars.color.textSecondary,
});

export const hypotheticalBadge = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  letterSpacing: "0.08em",
  color: vars.color.amber,
  border: `1px solid ${vars.color.amberBorder}`,
  background: vars.color.amberGlow,
  borderRadius: vars.radius.sm,
  padding: `2px ${vars.space.sm}`,
});

/** The permanent economy boundary. */
export const boundary = style({
  border: `1px solid ${vars.color.borderMid}`,
  background: vars.color.bgRaised,
  borderRadius: vars.radius.md,
  padding: vars.space.md,
  fontFamily: vars.font.mono,
  fontSize: "10.5px",
  lineHeight: 1.6,
  color: vars.color.textSecondary,
});

export const diffRow = style({
  display: "grid",
  gridTemplateColumns: "1fr auto auto",
  gap: vars.space.sm,
  alignItems: "baseline",
  padding: `${vars.space.xs} 0`,
  borderTop: `1px solid ${vars.color.border}`,
});

export const paramRow = style({
  display: "grid",
  gridTemplateColumns: "1fr 92px auto",
  gap: vars.space.sm,
  alignItems: "center",
  padding: `${vars.space.sm} 0`,
  borderTop: `1px solid ${vars.color.border}`,
});

/** One operator in the Console picker. */
export const opRow = style({
  display: "grid",
  gridTemplateColumns: "1fr auto",
  gap: vars.space.sm,
  alignItems: "baseline",
  padding: `${vars.space.sm}`,
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
  cursor: "pointer",
  selectors: { "&:hover": { background: vars.color.bgRaised } },
});

export const opRowActive = style([opRow, { background: vars.color.amberGlow }]);

export const pawaMeasured = style({
  fontFamily: vars.font.mono,
  fontSize: "10.5px",
  color: vars.color.teal,
  fontVariantNumeric: "tabular-nums",
});

export const pawaUnmeasured = style({
  fontFamily: vars.font.mono,
  fontSize: "10.5px",
  color: vars.color.textDim,
});

/* -- V1.5 ---------------------------------------------------------------- */

export const unavailableBadge = style({
  fontFamily: vars.font.mono,
  fontSize: "10px",
  letterSpacing: "0.08em",
  color: vars.color.textMuted,
  border: `1px dashed ${vars.color.borderMid}`,
  borderRadius: vars.radius.sm,
  padding: `2px ${vars.space.sm}`,
});

export const absenceHeadline = style({
  fontFamily: vars.font.ui,
  fontSize: "14px",
  lineHeight: 1.55,
  color: vars.color.textPrimary,
  margin: 0,
});

export const avatarLarge = style({
  width: "44px",
  height: "44px",
  borderRadius: vars.radius.md,
  background: vars.color.amberGlow,
  border: `1px solid ${vars.color.amberBorder}`,
  color: vars.color.amber,
  fontFamily: vars.font.mono,
  fontSize: "16px",
  fontWeight: 500,
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
});
