/**
 * COMPOSITION — `⊕`, and two honest absences.
 *
 * ★★ The tree is **real and engine-checked**: `MonitorEngine::flatten_holarchy`
 * refuses a duplicate id, an unknown parent and a cycle.
 *
 * ★★★ Two capabilities the Python engine has and `sustena-core` does not, both
 * rendered as honest unavailable rather than approximated:
 *
 * - **roll-up ρ** — folding children into a parent aggregate.
 * - **`holon.transfer`** — an atomic, conserved move between two Sustains.
 *
 * The second is the more dangerous one to fake, and the screen says why.
 */
import { createMemo, For, Show } from "solid-js";
import {
  Absent,
  Caption,
  Card,
  Column,
  Empty,
  Label,
  Meta,
  Note,
  NoteRow,
  Row,
  Spacer,
  Split,
  Sym,
  Value,
  vars,
} from "../ui";
import { fmt } from "../lib/engine";
import { childrenOf, selectedSustain, world } from "../lib/live";

export default function Composition() {
  const live = () => selectedSustain();
  const kids = createMemo(() => (live() ? childrenOf(live()!.summary.id) : []));
  const parent = createMemo(() => {
    const p = live()?.summary.parent;
    return p ? world.sustains[p]?.summary : undefined;
  });

  return (
    <Split>
      <Column>
        <Card title="⊕ · the tree" right={<Meta>{live()?.summary.label ?? "—"}</Meta>}>
          <NoteRow tone={world.holds ? "ok" : "danger"}>
            <Show when={world.holds} fallback={<Caption>{world.holarchyReason}</Caption>}>
              <Value>tree holds</Value> <Meta>· {world.linked} linked</Meta>
              <Caption>
                validated by <code>MonitorEngine::flatten_holarchy</code> — no duplicate id, no
                unknown parent, no cycle
              </Caption>
            </Show>
          </NoteRow>

          <Note gap="lg">
            <Label>parent</Label>
          </Note>
          <Show when={parent()} fallback={<Empty>none · this is a root Sustain</Empty>}>
            {(p) => (
              <Row>
                <Value>{p().label}</Value>
                <Meta>{p().template}</Meta>
                <Spacer />
                <Meta>{p().events} events</Meta>
                <Value>{fmt(p().liquid)}</Value>
              </Row>
            )}
          </Show>

          <Note gap="lg">
            <Label>children · {kids().length}</Label>
          </Note>
          <Show when={kids().length > 0} fallback={<Empty>no children linked</Empty>}>
            <For each={kids()}>
              {(k) => (
                <Row>
                  <Value>{k.label}</Value>
                  <Meta>{k.template}</Meta>
                  <Spacer />
                  <Meta>{k.events} events</Meta>
                  <Value>{fmt(k.liquid)}</Value>
                </Row>
              )}
            </For>
          </Show>
        </Card>
      </Column>

      <Column>
        <Card title={<>roll-up <Sym>ρ</Sym></>}>
          <Absent title="not available">
            The engine holds and checks this tree, but it cannot yet fold a child's state into a
            parent aggregate. ρ exists in the Python engine and has not been ported to{" "}
            <code>sustena-core</code>.
            <br />
            <br />
            Nothing is summed. A household total computed by this app rather than by the engine
            would be a number with no rule behind it — and no way to tell you when it stopped being
            true.
          </Absent>
        </Card>

        <Card title="transfer between Sustains">
          <Absent title="not available">
            <code>holon.transfer</code> — an <strong>atomic, conserved</strong> move that debits one
            Sustain's pocket and credits another's, both committed together or neither — is in the
            Python engine and{" "}
            <strong>
              not in <code>sustena-core</code>
            </strong>
            .
            <br />
            <br />
            ★★★ <strong>This one is deliberately not approximated.</strong> The obvious host
            workaround — spend from one Sustain, then record income on the other — is two separate
            gated calls. If the second refuses, the money has left one household and arrived
            nowhere. That is not a transfer; it is a way to lose money that looks like a feature.
            <br />
            <br />
            <code>juul::transfer</code> does exist in the core — but that moves the{" "}
            <em>economy's internal unit</em> between principals, not a household's own money
            between Sustains. Using it here would be answering a different question.
          </Absent>
        </Card>
      </Column>
    </Split>
  );
}
