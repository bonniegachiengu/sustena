/**
 * MONITOR — live on the push channel.
 *
 * ★★★ Nothing here polls and nothing re-fetches on change. Every value reads
 * from the live store, which is filled by `events.committed`. When a pocket
 * moves, the cells bound to that pocket repaint and the rest of the screen does
 * not — that is Solid's fine-grained reactivity doing the work a virtual DOM
 * would otherwise redo.
 *
 * ★★ Every frame, meter, dot and empty line on this screen comes from
 * `src/ui`. The screen decides WHAT is true; the system decides how truth looks.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import {
  Caption,
  Card,
  Chip,
  Cluster,
  Column,
  Empty,
  Fill,
  Label,
  Meta,
  Meter,
  Note,
  NoteRow,
  Readout,
  Rollup,
  Split,
  Sym,
  Value,
  vars,
  sx as S,
} from "../ui";
import { fmt, pockets, type Pocket } from "../lib/engine";
import { childrenOf, selectedSustain } from "../lib/live";

/**
 * ★★ The attention threshold is a **UI policy, declared here**, not an engine
 * reading. The engine has a real urgency notion — `d(s,V)` over a declared
 * `Region` — but these templates declare invariants rather than intervals, and
 * inventing a Region to borrow its authority would be declaring thresholds
 * nobody chose. So: a stated rule over real numbers, labelled as ours.
 */
const ATTENTION_AT = 0.8;

type Sort = "urgency" | "balance";
type Tone = "ok" | "warn" | "danger";

const pct = (p: Pocket): number | null => (p.allocated > 0 ? p.spent / p.allocated : null);

export default function Monitor() {
  const [sort, setSort] = createSignal<Sort>("urgency");

  const live = () => selectedSustain();
  const rows = createMemo(() => {
    const list = pockets(live()?.state);
    const by = sort();
    return [...list].sort((a, b) => {
      if (by === "urgency") {
        // ★ An unmeasurable pocket (nothing allocated) sorts last rather than
        //   as calm — a null is not a zero.
        const pa = pct(a);
        const pb = pct(b);
        if (pa === null && pb === null) return a.name.localeCompare(b.name);
        if (pa === null) return 1;
        if (pb === null) return -1;
        return pb - pa;
      }
      return b.left - a.left;
    });
  });

  /** Real signals only: pockets past the declared threshold, and rules the engine says are broken. */
  const attention = createMemo(() => {
    const out: { kind: string; what: string; why: string; tone: Tone }[] = [];
    for (const c of live()?.constraints ?? []) {
      if (!c.holds) {
        out.push({ kind: "rule", what: c.id, why: c.reason || c.expression, tone: "danger" });
      }
    }
    for (const p of pockets(live()?.state)) {
      const f = pct(p);
      if (f !== null && f >= ATTENTION_AT) {
        out.push({
          kind: "pocket",
          what: p.name,
          why: `${Math.round(f * 100)}% spent · ${fmt(p.left)} of ${fmt(p.allocated)} left`,
          tone: f >= 1 ? "danger" : "warn",
        });
      }
    }
    return out;
  });

  const kids = createMemo(() => (live() ? childrenOf(live()!.summary.id) : []));

  return (
    <Split>
      {/* ── left ───────────────────────────────────────────────────────── */}
      <Column>
        <Card
          title="live state"
          right={
            <Cluster>
              <Chip active={sort() === "urgency"} onClick={() => setSort("urgency")}>
                urgency
              </Chip>
              <Chip active={sort() === "balance"} onClick={() => setSort("balance")}>
                balance
              </Chip>
            </Cluster>
          }
        >
          <Show when={live()} fallback={<Empty>nothing selected</Empty>}>
            {(l) => (
              <>
                <Readout label="liquid balance" big>
                  {fmt(l().summary.liquid)}
                </Readout>
                <Show when={rows().length > 0} fallback={<Empty>no pockets yet</Empty>}>
                  <For each={rows()}>
                    {(p) => {
                      const f = () => pct(p);
                      return (
                        <div class={S.pocketBlock}>
                          <div class={S.pocketHead}>
                            <Value>{p.name}</Value>
                            <Meta>{f() === null ? "—" : `${Math.round(f()! * 100)}%`}</Meta>
                            <Value>{fmt(p.left)}</Value>
                          </div>
                          <Meter fraction={f()} />
                          <Caption>
                            spent {fmt(p.spent)} of {fmt(p.allocated)}
                          </Caption>
                        </div>
                      );
                    }}
                  </For>
                </Show>
              </>
            )}
          </Show>
        </Card>

        <Card title="needs attention" right={<Meta>{attention().length}</Meta>}>
          <Show
            when={attention().length > 0}
            fallback={<Empty>nothing needs you · all rules hold, no pocket is strained</Empty>}
          >
            <For each={attention()}>
              {(a) => (
                <NoteRow tone={a.tone}>
                  <Value>{a.what}</Value> <Meta>{a.kind}</Meta>
                  <Caption>{a.why}</Caption>
                </NoteRow>
              )}
            </For>
          </Show>
          <Note>
            <Caption>
              A broken rule is the engine's own verdict. The {Math.round(ATTENTION_AT * 100)}%
              pocket threshold is this app's declared policy over real numbers — not an engine
              reading, and labelled so.
            </Caption>
          </Note>
        </Card>

        <Card title="⊕ · roll-up">
          <Meta>
            {kids().length} linked {kids().length === 1 ? "child" : "children"}
          </Meta>
          {/* ★★★ Real, and the engine's. `compute_rollup` folds this Sustain's
              own state and every linked child's into its declared aggregates,
              fresh on every read and never cached. The exclusions travel with
              the figure — see `AggregateReadout`. */}
          <Rollup rollup={live()?.rollup ?? null} />
        </Card>
      </Column>

      {/* ── right ──────────────────────────────────────────────────────── */}
      <Column>
        <Card title="V · the engine's verdict, now">
          <Show
            when={(live()?.constraints ?? []).length > 0}
            fallback={<Empty>no invariants declared</Empty>}
          >
            <For each={live()!.constraints}>
              {(c) => (
                <NoteRow tone={c.holds ? "ok" : "danger"}>
                  <Value>{c.id}</Value> <Meta>{c.holds ? "holds" : "broken"}</Meta>
                  <Caption>{c.expression}</Caption>
                  <Show when={!c.holds && c.reason}>
                    {(r) => (
                      <div class={S.caption} style={{ color: vars.color.danger }}>
                        {r()}
                      </div>
                    )}
                  </Show>
                </NoteRow>
              )}
            </For>
          </Show>
        </Card>

        <Card
          title="the event log · state = fold(this)"
          right={<Meta>{live()?.log.length ?? 0} entries</Meta>}
        >
          <Show when={(live()?.log ?? []).length > 0} fallback={<Empty>no log yet</Empty>}>
            <For each={[...(live()?.log ?? [])].reverse()}>
              {(e) => (
                <div class={S.logRow}>
                  <span class={S.seqCell}>#{e.seq}</span>
                  <Fill>
                    <Value>{e.operator}</Value>
                    <Show when={e.events.length > 0}>
                      <Caption>
                        <For each={e.events}>{(ev) => <span>↳ {ev.name} </span>}</For>
                      </Caption>
                    </Show>
                  </Fill>
                  <span class={S.seqCell}>
                    {e.mutations} mut{e.mutations === 1 ? "" : "s"}
                  </span>
                </div>
              )}
            </For>
          </Show>
          <Show when={(live()?.log ?? []).length > 0}>
            <Note>
              <Label>every entry above is a fold input</Label>
            </Note>
          </Show>
        </Card>
      </Column>
    </Split>
  );
}
