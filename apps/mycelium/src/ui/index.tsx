/**
 * The cockpit's component system.
 *
 * ★★★ One definition each, consumed everywhere. A screen imports from here and
 * gets the mission-control look, the honest-state vocabulary and the responsive
 * behaviour without re-deciding any of them.
 *
 * The set, and why each earns a place:
 *
 * - **Card** — the instrument frame. Every panel is made of them.
 * - **Verdict** — the gate result. It was re-spelled on three screens; a gate
 *   refusal rendered slightly differently in two places is a refusal a person
 *   learns to read twice.
 * - **Unavailable** — the designed absence. The most load-bearing component in
 *   the product: it is how the app tells the truth about itself.
 * - **Readout / Row / NoteRow / Meter / TelemetryCell** — the reading families.
 * - **Badge / Dot** — status, with fixed semantics (teal ok, amber warn,
 *   red danger, dashed absent).
 * - **Empty / ErrorState** — the honest-state vocabulary, one voice.
 */
import { For, Show, type JSX } from "solid-js";
import * as L from "./layout.css";
import * as S from "./ui.css";
import { vars } from "./tokens.css";
import { fmt, type AggregateDto, type RollupDto } from "../lib/engine";

export { vars, bp } from "./tokens.css";
export * as layout from "./layout.css";
export * as sx from "./ui.css";

export type Tone = "ok" | "warn" | "danger" | "idle" | "active";
export type Verdictish = "admitted" | "refused" | "deferred";

/* ── layout primitives ──────────────────────────────────────────────────── */

export function Split(props: {
  variant?: "even" | "wideLeft" | "wideRight";
  children: JSX.Element;
}) {
  return <div class={L.split[props.variant ?? "even"]}>{props.children}</div>;
}

export function Column(props: { children: JSX.Element }) {
  return <div class={L.column}>{props.children}</div>;
}

export function Stack(props: { gap?: "xs" | "sm" | "md" | "lg"; children: JSX.Element }) {
  return <div class={L.stack[props.gap ?? "md"]}>{props.children}</div>;
}

export function Cluster(props: { gap?: "sm" | "md"; children: JSX.Element }) {
  return <div class={L.cluster[props.gap ?? "sm"]}>{props.children}</div>;
}

export function Cols(props: { children: JSX.Element }) {
  return <div class={L.cols}>{props.children}</div>;
}

/* ── card ───────────────────────────────────────────────────────────────── */

export function Card(props: {
  title: JSX.Element;
  right?: JSX.Element;
  scroll?: boolean;
  children: JSX.Element;
}) {
  return (
    <section class={S.card}>
      <header class={S.cardHead}>
        <span class={S.cardTitle}>{props.title}</span>
        <span class={S.spacer} />
        {props.right}
      </header>
      <div class={props.scroll ? S.cardBodyScroll : S.cardBody}>{props.children}</div>
    </section>
  );
}

/* ── the honest-state vocabulary ────────────────────────────────────────── */

/**
 * ★★ Every "nothing here" in the product. Lowercase, quiet, one line, and it
 * says WHY rather than only that there is nothing.
 */
export function Empty(props: { children: JSX.Element }) {
  return <div class={S.emptyLine}>{props.children}</div>;
}

/** Something genuinely went wrong — distinct from a refusal, always. */
export function ErrorState(props: { children: JSX.Element }) {
  return <div class={S.errorBox}>{props.children}</div>;
}

/** A capability this engine does not have. Dashed, and it names what is absent. */
export function Absent(props: { title: JSX.Element; children: JSX.Element }) {
  return (
    <div class={S.absence}>
      <span class={S.absenceTitle}>{props.title}</span>
      {props.children}
    </div>
  );
}

/** A hypothetical. Amber and dashed — never allowed to look like a fact. */
export function Hypothetical(props: { title?: JSX.Element; children: JSX.Element }) {
  return (
    <div class={S.hypothetical}>
      <Show when={props.title}>{(t) => <span class={S.absenceTitle}>{t()}</span>}</Show>
      {props.children}
    </div>
  );
}

/** A standing fact that bounds the product. Solid, because it is not a hole. */
export function Boundary(props: { title: JSX.Element; children: JSX.Element }) {
  return (
    <div class={S.boundary}>
      <span class={S.absenceTitle}>{props.title}</span>
      {props.children}
    </div>
  );
}

/* ── readings ───────────────────────────────────────────────────────────── */

/** A block set off from what precedes it — the cockpit's only vertical spacer. */
export function Note(props: { gap?: "sm" | "md" | "lg"; children: JSX.Element }) {
  return <div class={S.note[props.gap ?? "md"]}>{props.children}</div>;
}

/**
 * A span that may shrink. ★ Without `min-width: 0` a grid or flex child refuses
 * to go below its content width and pushes the page sideways — this is the one
 * definition of that fix.
 */
export function Fill(props: { children: JSX.Element }) {
  return <span class={S.fill}>{props.children}</span>;
}

/** A symbol held out of an uppercasing label — see `sym`. */
export function Sym(props: { children: JSX.Element }) {
  return <span class={S.sym}>{props.children}</span>;
}

export function Label(props: { children: JSX.Element }) {
  return <span class={S.label}>{props.children}</span>;
}

export function Meta(props: { children: JSX.Element }) {
  return <span class={S.meta}>{props.children}</span>;
}

export function Caption(props: { children: JSX.Element }) {
  return <div class={S.caption}>{props.children}</div>;
}

export function Value(props: { big?: boolean; tone?: Tone; children: JSX.Element }) {
  const colour = () =>
    props.tone === "ok"
      ? vars.color.teal
      : props.tone === "warn"
        ? vars.color.warn
        : props.tone === "danger"
          ? vars.color.danger
          : props.tone === "idle"
            ? vars.color.textDim
            : undefined;
  return (
    <span class={props.big ? S.value.big : S.value.base} style={{ color: colour() }}>
      {props.children}
    </span>
  );
}

/** label ↔ value. The cockpit's most common line. */
export function Readout(props: { label: string; big?: boolean; tone?: Tone; children: JSX.Element }) {
  return (
    <div class={S.readout}>
      <Label>{props.label}</Label>
      <Value big={props.big} tone={props.tone}>
        {props.children}
      </Value>
    </div>
  );
}

export function Row(props: { onClick?: () => void; children: JSX.Element }) {
  return props.onClick ? (
    <button class={S.rowButton} onClick={props.onClick}>
      {props.children}
    </button>
  ) : (
    <div class={S.row}>{props.children}</div>
  );
}

/** A dot and a block of prose that wraps beside it. */
export function NoteRow(props: { tone: Tone; children: JSX.Element }) {
  return (
    <div class={S.noteRow}>
      <span class={`${S.dot[props.tone]} ${S.noteDotShift}`} />
      <span style={{ "min-width": 0 }}>{props.children}</span>
    </div>
  );
}

/** Pushes what follows to the far end of a `Row`, `Cluster` or card header. */
export function Spacer() {
  return <span class={S.spacer} />;
}

export function Dot(props: { tone: Tone }) {
  return <span class={S.dot[props.tone]} />;
}

export function Badge(props: { tone: "ok" | "warn" | "danger" | "absent" | "quiet"; children: JSX.Element }) {
  return <span class={S.badge[props.tone]}>{props.children}</span>;
}

/**
 * A fill drawn from real numbers.
 *
 * ★ `fraction === null` means **unmeasurable**, not zero — the track renders
 * empty in the idle colour rather than a confident green.
 */
export function Meter(props: { fraction: number | null }) {
  const tone = () => {
    const f = props.fraction;
    if (f === null) return vars.color.borderLight;
    if (f >= 1) return vars.color.danger;
    if (f >= 0.8) return vars.color.warn;
    return vars.color.teal;
  };
  return (
    <div class={S.meterTrack}>
      <div
        class={S.meterFill}
        style={{ width: `${Math.min(100, (props.fraction ?? 0) * 100)}%`, background: tone() }}
      />
    </div>
  );
}

export function TelemetryStrip(props: { children: JSX.Element }) {
  return <div class={S.telemetryStrip}>{props.children}</div>;
}

export function TelemetryCell(props: { label: string; tone?: Tone; children: JSX.Element }) {
  return (
    <div class={S.telemetryCell}>
      <Label>{props.label}</Label>
      <Value tone={props.tone}>{props.children}</Value>
    </div>
  );
}

/* ── roll-up ρ ──────────────────────────────────────────────────────────── */

/**
 * ★★★ One declared aggregate, as the ENGINE answered it — and the exclusions
 * beside it, always.
 *
 * The whole reason ρ is worth having is that it refuses to fabricate. Two
 * refusals travel through this component and neither is optional:
 *
 * - **A member that could not be read is named, never counted as zero.** A
 *   silent drop is arithmetically indistinguishable from a member who genuinely
 *   holds nothing, so the excluded list renders whenever it is non-empty and it
 *   carries the engine's own reason.
 * - **`grounded: false` prints a dash, not a number.** `sum` over nothing is
 *   `0`, which is correct arithmetic and still not a measurement — a confident
 *   zero for a household nobody could read is the exact lie this replaced.
 *
 * One definition, three screens: the Monitor, the Constellation and
 * Composition all show the same figure the same way, so a household total
 * cannot appear to differ depending on where you look at it.
 */
export function AggregateReadout(props: { reading: AggregateDto; big?: boolean }) {
  const members = () =>
    props.reading.included.filter((c) => !c.isHousehold).length;
  return (
    <div class={S.note.sm}>
      <div class={S.readout}>
        <Label>{props.reading.id.replace(/_/g, " ")}</Label>
        <Show
          when={props.reading.grounded}
          fallback={
            <Value big={props.big} tone="idle">
              —
            </Value>
          }
        >
          <Value big={props.big}>{fmt(props.reading.value)}</Value>
        </Show>
      </div>
      <Caption>
        {props.reading.op.toLowerCase()} over{" "}
        <code>{props.reading.childPath}</code>
        <Show when={props.reading.grounded}>
          {" · "}
          {props.reading.includesHouseholdOwn ? "this Sustain's own" : "children"}
          {members() > 0 ? ` + ${members()} member${members() === 1 ? "" : "s"}` : ""}
        </Show>
        <Show when={!props.reading.grounded}>
          {" · nothing readable contributed — a 0 here would be a claim, not a measurement"}
        </Show>
      </Caption>
      <Show when={props.reading.excluded.length > 0}>
        <div class={S.note.sm}>
          <Absent title={`${props.reading.excluded.length} not counted`}>
            <For each={props.reading.excluded}>
              {(x) => (
                <div>
                  <strong>{x.label}</strong> — {x.reason}
                </div>
              )}
            </For>
            The figure above is the honest total of what <em>was</em> readable.
          </Absent>
        </div>
      </Show>
    </div>
  );
}

/**
 * A whole roll-up. ★ `declared nothing` is a real, separate state from
 * `could not compute` — a Sustain that asked for no totals is not a failure.
 */
export function Rollup(props: { rollup: RollupDto | null; big?: boolean }) {
  return (
    <Show when={props.rollup} fallback={<Empty>ρ has not been read for this Sustain yet</Empty>}>
      {(r) => (
        <Show
          when={r().aggregates.length > 0}
          fallback={<Empty>this Sustain declares no totals · nothing to fold</Empty>}
        >
          <For each={r().aggregates}>
            {(a) => <AggregateReadout reading={a} big={props.big} />}
          </For>
        </Show>
      )}
    </Show>
  );
}

/* ── the verdict ────────────────────────────────────────────────────────── */

export type VerdictProps = {
  verdict: Verdictish;
  operator: string;
  reason?: string | null;
  rule?: string | null;
  mutations: number;
  events: number;
  /** The line that says what the verdict MEANT for the world. */
  consequence?: string;
};

/**
 * ★★★ The gate result — one definition, every screen.
 *
 * The reason is the engine's own words and is never re-phrased: it names the
 * rule and the numbers, and softening it would delete the only useful part.
 */
export function Verdict(props: VerdictProps) {
  const word = () => props.verdict.toUpperCase();
  return (
    <div class={S.verdictBox[props.verdict]}>
      <Cluster>
        <span class={S.verdictWord[props.verdict]}>{word()}</span>
        <Meta>{props.operator}</Meta>
      </Cluster>
      <Show when={props.reason}>{(r) => <p class={S.reason}>{r()}</p>}</Show>
      <Show when={props.rule}>{(c) => <div class={S.reasonCode}>rule · {c()}</div>}</Show>
      <div class={S.reasonCode}>
        {props.mutations} mutation{props.mutations === 1 ? "" : "s"} · {props.events} event
        {props.events === 1 ? "" : "s"}
        <Show when={props.consequence}>{(c) => <> · {c()}</>}</Show>
      </div>
    </div>
  );
}

/* ── the designed absence ───────────────────────────────────────────────── */

export type Absence = {
  title: JSX.Element;
  headline: string;
  present: string[];
  missing: string[];
  where: string;
  refusal?: string;
};

/**
 * ★★★ A panel whose subsystem is not in `sustena-core` still gets the cockpit's
 * layout and a legible account of **what exists, what does not, and where the
 * capability lives**. A blank "coming soon" tells a person nothing; this tells
 * them the truth about the system they are running.
 */
export function Unavailable(props: { absence: Absence; children?: JSX.Element }) {
  return (
    <Split>
      <Column>
        <Card title={props.absence.title} right={<Badge tone="absent">not in this engine</Badge>}>
          <p class={S.prose}>{props.absence.headline}</p>

          <Stack gap="xs">
            <div style={{ "margin-top": vars.space.lg }}>
              <Label>what the core does have</Label>
            </div>
            <For each={props.absence.present}>
              {(x) => (
                <NoteRow tone="ok">
                  <Caption>{x}</Caption>
                </NoteRow>
              )}
            </For>

            <div style={{ "margin-top": vars.space.md }}>
              <Label>what it does not</Label>
            </div>
            <For each={props.absence.missing}>
              {(x) => (
                <NoteRow tone="idle">
                  <Caption>{x}</Caption>
                </NoteRow>
              )}
            </For>
          </Stack>

          <div style={{ "margin-top": vars.space.lg }}>
            <Absent title="where it lives">{props.absence.where}</Absent>
          </div>

          <Show when={props.absence.refusal}>
            {(r) => (
              <div style={{ "margin-top": vars.space.md }}>
                <Absent title="why this is not approximated">{r()}</Absent>
              </div>
            )}
          </Show>
        </Card>
      </Column>
      <Column>{props.children}</Column>
    </Split>
  );
}

/* ── controls ───────────────────────────────────────────────────────────── */

export function Button(props: {
  variant?: "primary" | "ghost";
  disabled?: boolean;
  onClick?: () => void;
  children: JSX.Element;
}) {
  return (
    <button
      class={S.button[props.variant ?? "primary"]}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

/**
 * A small control. With `active` it is the cockpit's tab — one mechanism, so a
 * "which view am I in" control looks the same on every screen that has one.
 */
export function Chip(props: {
  active?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  children: JSX.Element;
}) {
  return (
    <button
      class={props.active ? S.chipActive : S.chip}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

export function Field(props: { label: string; children: JSX.Element }) {
  return (
    <div class={S.field}>
      <Label>{props.label}</Label>
      {props.children}
    </div>
  );
}

export function Code(props: { children: JSX.Element }) {
  return <pre class={S.codeBlock}>{props.children}</pre>;
}
