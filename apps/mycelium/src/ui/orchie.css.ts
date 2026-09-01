/**
 * ORCHIE'S OWN SCALE — because a cockpit and a phone are not the same instrument.
 *
 * ★★★ **The root cause of "clunky and impossible to use".** Orchie was rendered
 * with Mycelium's tokens, and those are a *cockpit* scale: body text at 11.5px,
 * labels at 10px, chips with 4px/8px padding. On a 1080-wide phone held at
 * arm's length that is roughly a 22px tall tap target carrying 10px text. The
 * platform guidance is 48dp, and the reason is not aesthetic -- a thumb pad is
 * about 45px across, so a 22px target is smaller than the finger pressing it
 * and you aim by memory. That is precisely what "fights the finger" means.
 *
 * ★★ These are Orchie-only. Mycelium keeps its density, because on a laptop
 * with a mouse that density is an advantage and shrinking it would be a
 * regression for the person who actually uses it that way. Same tokens, same
 * palette, same voice, different scale -- the colours below are all `vars`, so
 * the corrected WCAG contrast ramp is inherited rather than re-decided.
 *
 * ★★★ **Touch feedback is `:active`, not `:hover`.** Every interactive style in
 * the cockpit reacts on hover, which on a touchscreen either never fires or
 * fires and then STICKS after the finger leaves. So a tapped chip stayed lit
 * and an untapped one gave nothing back. Here every control has an `:active`
 * state that fires on the way down, and `touch-action: manipulation` removes
 * the browser's double-tap-zoom wait -- that wait is a real ~300ms of "nothing
 * happened" on every single tap, and it is most of what "slow" felt like.
 */
import { globalStyle, keyframes, style, styleVariants } from "@vanilla-extract/css";
import { vars } from "./tokens.css";

// ═══════════════════════════════════════════════════════════════════════════
// Global touch behaviour. Scoped to the Orchie frame via a data attribute so
// the cockpit is untouched.
// ═══════════════════════════════════════════════════════════════════════════

globalStyle('[data-face="orchie"]', {
  // The grey flash Android paints over a tapped element, on its own schedule,
  // after its own delay. Replaced below with feedback we control.
  WebkitTapHighlightColor: "transparent",
  // Stops long-press turning a card into a text selection while scrolling.
  WebkitUserSelect: "none",
  userSelect: "none",
});

// Text a person may genuinely want to copy keeps selection.
globalStyle('[data-face="orchie"] p, [data-face="orchie"] code', {
  WebkitUserSelect: "text",
  userSelect: "text",
});

globalStyle('[data-face="orchie"] button, [data-face="orchie"] input', {
  // No double-tap-zoom wait. See the header note: this is a real 300ms.
  touchAction: "manipulation",
});

// ═══════════════════════════════════════════════════════════════════════════
// Motion
// ═══════════════════════════════════════════════════════════════════════════

const rise = keyframes({
  from: { opacity: 0, transform: "translateY(6px)" },
  to: { opacity: 1, transform: "none" },
});

/**
 * ★★★ **`opacity`, not `background-position`.**
 *
 * The first version animated a gradient's `background-position`, which is a
 * PAINT property: the browser re-rasterises the element on every frame, on the
 * main render thread, for as long as it runs. Measured on the device while the
 * feed was stalled, that shimmer alone held the process at ~53% CPU and had
 * burned over four minutes of CPU time -- a loading indicator that made the
 * thing it was waiting for slower, and cooked the phone doing it.
 *
 * `opacity` is a COMPOSITED property. It runs on the compositor without
 * repainting anything, which is why this is the one safe way to animate an
 * element that may be on screen indefinitely.
 */
const shimmer = keyframes({
  "0%, 100%": { opacity: 0.45 },
  "50%": { opacity: 0.9 },
});

const pulse = keyframes({
  "0%, 100%": { opacity: 1 },
  "50%": { opacity: 0.45 },
});

/**
 * ★★ Every animation here is behind this guard. Motion that cannot be turned
 * off is an accessibility failure, and on a phone it is also a battery cost.
 */
globalStyle("@media (prefers-reduced-motion: reduce)", {});

// ═══════════════════════════════════════════════════════════════════════════
// The frame
// ═══════════════════════════════════════════════════════════════════════════

/**
 * ★★★ The scroll container, and the one place the keyboard is accounted for.
 *
 * `--kb` is published by lib/viewport.ts. Adding it to the bottom padding means
 * the content can always be scrolled clear of the keyboard: without it, the
 * last card is unreachable while typing, which is the other half of the bug.
 */
export const frame = style({
  position: "fixed",
  inset: 0,
  overflowY: "auto",
  overflowX: "hidden",
  WebkitOverflowScrolling: "touch",
  // A pull past the end must not drag the whole WebView (rubber-banding the
  // app itself looks broken) and must not trigger pull-to-refresh.
  overscrollBehavior: "contain",
  background: vars.color.bgBase,
  paddingTop: "max(14px, env(safe-area-inset-top))",
  paddingLeft: "max(14px, env(safe-area-inset-left))",
  paddingRight: "max(14px, env(safe-area-inset-right))",
  // safe area + keyboard + room to breathe past the last card.
  paddingBottom: "calc(env(safe-area-inset-bottom) + var(--kb, 0px) + 96px)",
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
});

export const column = style({
  width: "100%",
  maxWidth: "560px",
  display: "flex",
  flexDirection: "column",
  gap: "14px",
  minWidth: 0,
});

/** The header. Compact, and it gets out of the way while typing. */
export const header = style({
  display: "flex",
  alignItems: "center",
  gap: "10px",
  paddingBottom: "2px",
  transition: "opacity 160ms ease, max-height 200ms ease",
  selectors: {
    '[data-keyboard="open"] &': { opacity: 0, maxHeight: 0, overflow: "hidden", paddingBottom: 0 },
  },
});

export const brand = style({
  fontFamily: vars.font.mono,
  fontSize: "13px",
  fontWeight: 500,
  letterSpacing: "0.22em",
  color: vars.color.amber,
});

/**
 * ★★★ The way OUT of Orchie, on a phone.
 *
 * The face toggle lives in Mycelium's topbar -- and `frame` above is
 * `position: fixed; inset: 0`, so on a phone Orchie paints straight over it.
 * The cockpit was documented as "one tap away" and was in fact unreachable:
 * there was no way to open Ingest (paste a real M-Pesa/KCB message) from the
 * device the messages actually arrive on. This is that tap.
 *
 * ★★ Orchie's scale, not the cockpit's -- 44px, for the reason this whole file
 * exists. The cockpit's own chip is ~22px, which is smaller than the thumb
 * pressing it.
 */
export const faceToggle = style({
  fontFamily: vars.font.mono,
  fontSize: "11px",
  letterSpacing: "0.12em",
  minHeight: "44px",
  padding: "0 14px",
  display: "inline-flex",
  alignItems: "center",
  color: vars.color.textSecondary,
  background: vars.color.bgRaised,
  border: `1px solid ${vars.color.border}`,
  borderRadius: "10px",
  cursor: "pointer",
  whiteSpace: "nowrap",
  flexShrink: 0,
  selectors: {
    "&:active": { color: vars.color.textPrimary, borderColor: vars.color.amberBorder },
  },
});

export const headerMeta = style({
  fontFamily: vars.font.mono,
  fontSize: "11px",
  color: vars.color.textMuted,
  marginLeft: "auto",
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
  maxWidth: "55%",
});

// ═══════════════════════════════════════════════════════════════════════════
// Cards
// ═══════════════════════════════════════════════════════════════════════════

export const card = style({
  background: vars.color.bgSurface,
  border: `1px solid ${vars.color.border}`,
  borderRadius: "10px",
  padding: "14px",
  display: "flex",
  flexDirection: "column",
  gap: "10px",
  minWidth: 0,
  animation: `${rise} 240ms ease-out both`,
  "@media": { "(prefers-reduced-motion: reduce)": { animation: "none" } },
});

/** The one card that is the job in front of you. It should look like it. */
export const cardPrimary = style([
  card,
  {
    borderColor: vars.color.amberBorder,
    background: `linear-gradient(${vars.color.amberGlow}, transparent 60%), ${vars.color.bgSurface}`,
  },
]);

export const cardTitle = style({
  fontFamily: vars.font.ui,
  fontSize: "16px",
  fontWeight: 600,
  lineHeight: 1.3,
  color: vars.color.textPrimary,
  margin: 0,
});

export const cardHead = style({ display: "flex", alignItems: "flex-start", gap: "10px" });

/** ★ 16px minimum. Below that is where a phone stops being readable to an
 *  adult who is not looking for an excuse to squint. */
export const body = style({
  fontFamily: vars.font.ui,
  fontSize: "15px",
  lineHeight: 1.5,
  color: vars.color.textSecondary,
  margin: 0,
});

export const caption = style({
  fontFamily: vars.font.ui,
  fontSize: "13px",
  lineHeight: 1.45,
  color: vars.color.textMuted,
  margin: 0,
});

/** A number you are meant to read across the room. */
export const figure = style({
  fontFamily: vars.font.mono,
  fontSize: "30px",
  fontWeight: 500,
  letterSpacing: "-0.01em",
  color: vars.color.textPrimary,
  fontVariantNumeric: "tabular-nums",
});

export const figureLabel = style({
  fontFamily: vars.font.mono,
  fontSize: "11px",
  letterSpacing: "0.14em",
  textTransform: "uppercase",
  color: vars.color.textMuted,
});

// ═══════════════════════════════════════════════════════════════════════════
// Controls -- all of them at least 48px in the direction a thumb lands
// ═══════════════════════════════════════════════════════════════════════════

const tappable = style({
  fontFamily: vars.font.ui,
  minHeight: "52px",
  padding: "14px 18px",
  borderRadius: "10px",
  border: "1px solid transparent",
  fontSize: "16px",
  fontWeight: 500,
  cursor: "pointer",
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  gap: "8px",
  // ★ The feedback is a scale, not a colour change: it reads instantly even
  //   under a thumb that is covering most of the control.
  transition: "transform 90ms ease, background 140ms ease, border-color 140ms ease, opacity 140ms",
  selectors: {
    "&:active:not(:disabled)": { transform: "scale(0.97)" },
    "&:disabled": { opacity: 0.5, cursor: "default" },
  },
  "@media": { "(prefers-reduced-motion: reduce)": { transition: "background 140ms ease" } },
});

export const action = styleVariants({
  /** The one thing to do next. */
  primary: [
    tappable,
    {
      color: "#1a1200",
      background: vars.color.amber,
      fontWeight: 600,
      selectors: { "&:active:not(:disabled)": { transform: "scale(0.97)", background: vars.color.amberDim } },
    },
  ],
  /** A real alternative, not a decoration. */
  secondary: [
    tappable,
    {
      color: vars.color.textPrimary,
      background: vars.color.bgRaised,
      borderColor: vars.color.borderMid,
      selectors: { "&:active:not(:disabled)": { transform: "scale(0.97)", background: vars.color.bgOverlay } },
    },
  ],
  /** Getting out. Quiet, but the same size -- a cancel you can't hit is a trap. */
  quiet: [
    tappable,
    {
      color: vars.color.textMuted,
      background: "transparent",
      borderColor: vars.color.border,
      selectors: { "&:active:not(:disabled)": { transform: "scale(0.97)", color: vars.color.textPrimary } },
    },
  ],
});

export const actionWide = style({ width: "100%" });

/**
 * ★★★ The answer buttons -- the thing his mum taps most.
 *
 * A grid, not a wrapping row: wrapped chips give you a ragged last line and
 * targets of six different widths, so the eye has to search. Equal cells at a
 * fixed height are scannable, and at two columns each cell is ~50% of a phone's
 * width, which is a target you can hit without looking.
 */
export const options = style({
  display: "grid",
  gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))",
  gap: "10px",
});

/**
 * The pocket picker, and only it.
 *
 * ★★★ A pocket list is a different shape of question from "what should this
 * do?". That one has two or three answers and each deserves a proper target;
 * this one has as many answers as he has pockets, and rendering them all as
 * full-size buttons turns one question into a wall to read. Chips wrap, so
 * fifteen pockets are a paragraph rather than a page.
 *
 * ★★ Still a real target. 34px is above the floor where a thumb starts
 * missing, and the wrapping row keeps neighbours far enough apart that a near
 * miss lands on nothing rather than on the wrong pocket -- which matters more
 * here than size does, because the wrong pocket is a wrong record.
 */
/**
 * Constraint health, as a colour on the card's own edge.
 *
 * ★★★ Monitor §VII rests on Treisman's Feature Integration Theory: some visual
 * attributes are processed in parallel across the whole field BEFORE attention
 * engages, in roughly 150 to 200ms. Hue is one of them. The ranking was already
 * being computed correctly and then drawn flat — every card in one weight,
 * whatever its score — so it existed in the data and never reached the eye.
 *
 * ★★ On the leading edge rather than the whole card. A tinted background
 * competes with the text sitting on it, and the point is to be readable at a
 * glance without making anything harder to read up close.
 *
 * ★★ Never the only signal. A colour alone would be invisible to anyone who
 * cannot separate these hues, so every card that carries one also says the
 * same thing in words. This makes the glance faster; it does not carry meaning
 * on its own.
 */
const edge = (color: string) =>
  style({
    borderLeftWidth: "3px",
    borderLeftStyle: "solid",
    borderLeftColor: color,
  });

export const healthGreen = edge(vars.color.teal);
export const healthAmber = edge(vars.color.amber);
export const healthRed = edge(vars.color.danger);

export const chips = style({
  display: "flex",
  flexWrap: "wrap",
  gap: "5px",
  alignItems: "center",
});

export const chip = style({
  fontFamily: vars.font.ui,
  fontSize: "12px",
  fontWeight: 500,
  // ★★ 28px. Below the 44px a lone button wants, and deliberately so: these
  //    are not lone buttons. Fifteen of them at button size is a page to read
  //    before choosing, and reading is what actually costs him here — the tap
  //    is easy either way once he has found the word.
  minHeight: "28px",
  padding: "4px 9px",
  borderRadius: "999px",
  color: vars.color.textPrimary,
  background: vars.color.bgRaised,
  border: `1px solid ${vars.color.borderMid}`,
  cursor: "pointer",
  textAlign: "center",
  lineHeight: 1.2,
  whiteSpace: "nowrap",
  // ★★ A long name is trimmed rather than allowed to set the row's width.
  //    "Miscellaneous" was taking a whole line to itself and pushing three
  //    short names onto the next one.
  maxWidth: "10rem",
  overflow: "hidden",
  textOverflow: "ellipsis",
  transition: "transform 90ms ease, background 140ms ease, border-color 140ms ease",
  selectors: {
    "&:active:not(:disabled)": {
      transform: "scale(0.94)",
      background: vars.color.amberGlow,
      borderColor: vars.color.amberBorder,
    },
    "&:disabled": { opacity: 0.4, cursor: "default" },
  },
});

export const chipChosen = style([
  chip,
  {
    background: vars.color.amberGlow,
    borderColor: vars.color.amberBorder,
    color: vars.color.amber,
  },
]);

/**
 * A pocket that is somebody.
 *
 * ★★★ Drawn differently because it BEHAVES differently: it runs both ways and
 * can sit in his favour, which no envelope does. A person and a category that
 * look identical in the picker teach that they are the same kind of thing, and
 * the first time that matters is the moment he files a repayment as shopping.
 *
 * ★★ A teal rim rather than a second amber. Amber already means "chosen" three
 * lines below, and two meanings on one colour is how a marker stops being read.
 */
export const chipPerson = style([
  chip,
  {
    borderColor: vars.color.tealBorder,
    color: vars.color.teal,
  },
]);

/** The odd one out: creating a pocket is a different act from picking one. */
export const chipNew = style([
  chip,
  {
    background: "transparent",
    borderStyle: "dashed",
    color: vars.color.textMuted,
  },
]);

export const option = style({
  fontFamily: vars.font.ui,
  fontSize: "16px",
  fontWeight: 500,
  minHeight: "56px",
  padding: "12px 14px",
  borderRadius: "10px",
  color: vars.color.textPrimary,
  background: vars.color.bgRaised,
  border: `1px solid ${vars.color.borderMid}`,
  cursor: "pointer",
  textAlign: "center",
  lineHeight: 1.25,
  wordBreak: "break-word",
  transition: "transform 90ms ease, background 140ms ease, border-color 140ms ease",
  selectors: {
    "&:active:not(:disabled)": {
      transform: "scale(0.96)",
      background: vars.color.amberGlow,
      borderColor: vars.color.amberBorder,
    },
    "&:disabled": { opacity: 0.4, cursor: "default" },
  },
  "@media": { "(prefers-reduced-motion: reduce)": { transition: "background 140ms ease" } },
});

/** ★ The option that was just chosen, held lit while the engine answers. */
export const optionChosen = style([
  option,
  { background: vars.color.amberGlow, borderColor: vars.color.amberBorder, color: vars.color.amber },
]);

/**
 * ★★★ 16px is not a style choice. Any input under 16px makes mobile browsers
 * zoom the page on focus, and the zoom does not undo itself -- you finish
 * typing and the whole app is left magnified and scrolled sideways. That alone
 * reads as "broken".
 */
export const input = style({
  fontFamily: vars.font.ui,
  fontSize: "16px",
  minHeight: "52px",
  width: "100%",
  color: vars.color.textPrimary,
  background: vars.color.bgBase,
  border: `1px solid ${vars.color.borderMid}`,
  borderRadius: "10px",
  padding: "13px 15px",
  outline: "none",
  transition: "border-color 140ms ease, box-shadow 140ms ease",
  selectors: {
    "&:focus": {
      borderColor: vars.color.amberBorder,
      boxShadow: `0 0 0 3px ${vars.color.amberGlow}`,
    },
    "&::placeholder": { color: vars.color.textDim },
  },
});

export const inputNumeric = style([input, { fontFamily: vars.font.mono, fontVariantNumeric: "tabular-nums" }]);

/** A small, genuinely secondary control. Still 44px tall. */
export const linkish = style({
  fontFamily: vars.font.ui,
  fontSize: "14px",
  minHeight: "44px",
  padding: "10px 12px",
  background: "transparent",
  border: "none",
  color: vars.color.textMuted,
  textAlign: "left",
  cursor: "pointer",
  borderRadius: "8px",
  transition: "color 140ms ease, background 140ms ease",
  selectors: { "&:active": { color: vars.color.textPrimary, background: vars.color.bgRaised } },
});

export const row = style({ display: "flex", alignItems: "center", gap: "10px", minWidth: 0 });
export const stack = style({ display: "flex", flexDirection: "column", gap: "10px", minWidth: 0 });
export const spacer = style({ flex: 1 });

// ═══════════════════════════════════════════════════════════════════════════
// State: loading, empty, error, verdict
// ═══════════════════════════════════════════════════════════════════════════

/**
 * ★★★ A skeleton, not a spinner and not a blank.
 *
 * The old screen replaced everything with the words "reading the household…",
 * so every refresh threw the layout away and rebuilt it -- which reads as a
 * stall even when it takes 80ms. A skeleton in the shape of the answer keeps
 * the layout still and tells the eye where to wait.
 */
export const skeleton = style({
  borderRadius: "8px",
  // A flat fill rather than a gradient: nothing to re-rasterise.
  background: vars.color.bgRaised,
  animation: `${shimmer} 1.6s ease-in-out infinite`,
  // ★ Promotes it to its own compositor layer so the pulse never touches paint.
  willChange: "opacity",
  "@media": { "(prefers-reduced-motion: reduce)": { animation: "none", opacity: 0.6 } },
});

export const skelLine = styleVariants({
  title: [skeleton, { height: "18px", width: "58%" }],
  text: [skeleton, { height: "13px", width: "88%" }],
  short: [skeleton, { height: "13px", width: "44%" }],
  figure: [skeleton, { height: "34px", width: "50%" }],
});

/** ★ Shown over content that is being refreshed, so it dims rather than vanishes. */
export const staleWhileRefreshing = style({
  opacity: 0.55,
  transition: "opacity 180ms ease",
});

export const empty = style({
  fontFamily: vars.font.ui,
  fontSize: "15px",
  color: vars.color.textMuted,
  textAlign: "center",
  padding: "36px 16px",
  lineHeight: 1.5,
});

export const errorBox = style({
  fontFamily: vars.font.ui,
  fontSize: "14px",
  lineHeight: 1.5,
  color: vars.color.danger,
  background: vars.color.dangerGlow,
  border: `1px solid ${vars.color.dangerBorder}`,
  borderRadius: "10px",
  padding: "14px",
});

export const verdict = styleVariants({
  admitted: [
    { borderRadius: "10px", padding: "16px", textAlign: "center" as const },
    { background: vars.color.tealGlow, border: `1px solid ${vars.color.tealBorder}` },
  ],
  refused: [
    { borderRadius: "10px", padding: "16px", textAlign: "center" as const },
    { background: vars.color.dangerGlow, border: `1px solid ${vars.color.dangerBorder}` },
  ],
});

export const verdictWord = styleVariants({
  admitted: [{ fontFamily: vars.font.mono, fontSize: "17px", letterSpacing: "0.16em", fontWeight: 500 }, { color: vars.color.teal }],
  refused: [{ fontFamily: vars.font.mono, fontSize: "17px", letterSpacing: "0.16em", fontWeight: 500 }, { color: vars.color.danger }],
});

/** A dot that says "working", without taking a whole screen to say it. */
export const working = style({
  display: "inline-block",
  width: "8px",
  height: "8px",
  borderRadius: "50%",
  background: vars.color.amber,
  animation: `${pulse} 900ms ease-in-out infinite`,
  "@media": { "(prefers-reduced-motion: reduce)": { animation: "none" } },
});

export const badge = styleVariants({
  warn: [
    { fontFamily: vars.font.mono, fontSize: "11px", padding: "4px 8px", borderRadius: "6px", whiteSpace: "nowrap" as const },
    { color: vars.color.warn, background: vars.color.amberGlow, border: `1px solid ${vars.color.amberBorder}` },
  ],
  danger: [
    { fontFamily: vars.font.mono, fontSize: "11px", padding: "4px 8px", borderRadius: "6px", whiteSpace: "nowrap" as const },
    { color: vars.color.danger, background: vars.color.dangerGlow, border: `1px solid ${vars.color.dangerBorder}` },
  ],
  quiet: [
    { fontFamily: vars.font.mono, fontSize: "11px", padding: "4px 8px", borderRadius: "6px", whiteSpace: "nowrap" as const },
    { color: vars.color.textMuted, background: vars.color.bgRaised, border: `1px solid ${vars.color.border}` },
  ],
});

/** The raw message text, kept readable but clearly quoted machinery. */
export const raw = style({
  fontFamily: vars.font.mono,
  fontSize: "12.5px",
  lineHeight: 1.5,
  color: vars.color.textMuted,
  background: vars.color.bgBase,
  border: `1px solid ${vars.color.border}`,
  borderRadius: "8px",
  padding: "10px 12px",
  margin: 0,
  whiteSpace: "pre-wrap",
  wordBreak: "break-word",
  maxHeight: "132px",
  overflowY: "auto",
});

export const meter = style({
  height: "8px",
  borderRadius: "4px",
  background: vars.color.bgRaised,
  overflow: "hidden",
});

export const meterFill = style({
  height: "100%",
  borderRadius: "4px",
  transition: "width 320ms ease-out",
  "@media": { "(prefers-reduced-motion: reduce)": { transition: "none" } },
});

/** A progress trail through a multi-step question. Small, but it stops the
 *  flow feeling endless -- you can see there is a bottom to it. */
export const steps = style({ display: "flex", gap: "5px", alignItems: "center" });
export const stepDot = styleVariants({
  done: [{ width: "18px", height: "3px", borderRadius: "2px" }, { background: vars.color.amber }],
  todo: [{ width: "18px", height: "3px", borderRadius: "2px" }, { background: vars.color.border }],
});

/** The stay-unlocked toggle. ★ The caption is deliberately part of the label:
 *  a person agreeing to this is agreeing to a real trade, and the trade has to
 *  be readable at the moment of agreeing rather than in a help page. */
export const remember = style({
  display: "flex",
  alignItems: "flex-start",
  gap: "10px",
  padding: "10px 2px 2px",
  cursor: "pointer",
  fontSize: "13px",
  lineHeight: 1.45,
  color: vars.color.textMuted,
});

globalStyle(`${remember} em`, {
  display: "block",
  fontStyle: "normal",
  marginTop: "3px",
  fontSize: "11.5px",
  lineHeight: 1.45,
  color: vars.color.textDim,
});

globalStyle(`${remember} input`, { marginTop: "2px", flexShrink: 0 });

/**
 * One figure being taught, and its own decisions.
 *
 * ★★ Boxed and separated, because the mistake to design against is answering
 * for the wrong number. Two figures in one message need to look like two
 * questions, not one paragraph with several taps in it.
 */
export const figureRow = style({
  borderTop: `1px solid ${vars.color.border}`,
  paddingTop: 12,
  marginTop: 12,
  display: "flex",
  flexDirection: "column",
  gap: 6,
});

/** The number itself, large enough to check at a glance. */
export const figureValue = style({
  fontFamily: vars.font.mono,
  fontSize: 18,
  fontWeight: 600,
  color: vars.color.textPrimary,
});
