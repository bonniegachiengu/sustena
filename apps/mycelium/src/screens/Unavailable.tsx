/**
 * The designed unavailable state.
 *
 * ★★ A panel whose subsystem is not in `sustena-core` still gets the cockpit's
 * layout, its title, and a legible account of **what exists, what does not, and
 * where the capability actually lives**. A blank "coming soon" would tell a
 * person nothing; this tells them the truth about the system they are running.
 */
import { For, type JSX } from "solid-js";
import * as s from "../styles/app.css";
import { vars } from "../styles/tokens.css";

export type Absence = {
  title: string;
  /** The one-line statement of what is missing. */
  headline: string;
  /** What the engine genuinely does have, so the gap is placed rather than vague. */
  present: string[];
  /** What is missing, named precisely. */
  missing: string[];
  /** Where it lives instead, and what porting it would mean. */
  where: string;
  /** Anything that would be dishonest to fake, and why. */
  refusal?: string;
};

export default function Unavailable(props: { absence: Absence; children?: JSX.Element }) {
  return (
    <div class={s.main}>
      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>{props.absence.title}</span>
            <span class={s.spacer} />
            <span class={s.unavailableBadge}>not in this engine</span>
          </header>
          <div class={s.cardBody}>
            <p class={s.absenceHeadline}>{props.absence.headline}</p>

            <div class={s.label} style={{ "margin-top": vars.space.lg }}>what the core does have</div>
            <For each={props.absence.present}>
              {(x) => (
                <div class={s.invariantRow}>
                  <span class={s.dot} style={{ background: vars.color.teal, "margin-top": "5px" }} />
                  <span>{x}</span>
                </div>
              )}
            </For>

            <div class={s.label} style={{ "margin-top": vars.space.lg }}>what it does not</div>
            <For each={props.absence.missing}>
              {(x) => (
                <div class={s.invariantRow}>
                  <span class={s.dot} style={{ background: vars.color.textDim, "margin-top": "5px" }} />
                  <span>{x}</span>
                </div>
              )}
            </For>

            <div class={s.unavailable} style={{ "margin-top": vars.space.lg }}>
              <span class={s.unavailableTitle}>where it lives</span>
              {props.absence.where}
            </div>

            {props.absence.refusal ? (
              <div class={s.unavailable} style={{ "margin-top": vars.space.md }}>
                <span class={s.unavailableTitle}>why this is not approximated</span>
                {props.absence.refusal}
              </div>
            ) : null}
          </div>
        </section>
      </div>
      <div class={s.column}>{props.children}</div>
    </div>
  );
}
