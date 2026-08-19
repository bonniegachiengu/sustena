/**
 * PROFILE — identity, and the **real** capability model.
 *
 * ★★ The permission matrix is the engine's: `effective_privilege` walks the
 * membership path with the weakest-link rule, and `permitted` compares it
 * against each operator's declared `min_privilege`.
 *
 * ★★★ **And the gate is not checking any of it.** Every call in this host runs
 * `Authorization::Unchecked`. This screen shows the authority the principal
 * *holds*, not one being enforced — a permission matrix that implied
 * enforcement would be security theatre, so it says so at the top.
 */
import { createResource, For, Show } from "solid-js";
import {
  Absent,
  Badge,
  Caption,
  Card,
  Cluster,
  Column,
  Empty,
  Fill,
  Label,
  Meta,
  Note,
  Readout,
  Row,
  Spacer,
  Split,
  TelemetryCell,
  TelemetryStrip,
  Value,
  vars,
  sx as S,
} from "../ui";
import { engine } from "../lib/engine";
import { selectedSustain, world } from "../lib/live";

const TIER = ["owner", "member", "contributor", "observer"];
const tierName = (t: number | null | undefined) =>
  t === null || t === undefined ? "—" : (TIER[t] ?? `tier ${t}`);

export default function Profile() {
  const [access] = createResource(
    () => world.selected,
    (id) => engine.access(id),
  );

  const totalEvents = () =>
    world.order.reduce((n, k) => n + (world.sustains[k]?.summary.events ?? 0), 0);

  return (
    <Split>
      <Column>
        <Card title="identity">
          <Cluster gap="md">
            <span class={S.avatarLarge}>bg</span>
            <Fill>
              <Value big>{world.principal || "—"}</Value>
              <Caption>the local principal</Caption>
            </Fill>
          </Cluster>

          {/* ★★★ The honesty that matters most on this screen. */}
          <Note gap="lg">
            <Absent title="declared, not authenticated">
              There is no sign-in. This handle is a name the host declares and the ledger charges
              to — it is not proof of anything, and nothing checked it.
            </Absent>
          </Note>
        </Card>

        <Card title="activity">
          <TelemetryStrip>
            <TelemetryCell label="sustains">{world.order.length}</TelemetryCell>
            <TelemetryCell label="logged events">{totalEvents()}</TelemetryCell>
            <TelemetryCell label="pushed this session">{world.pushes}</TelemetryCell>
            <TelemetryCell label="refused this session">{world.refusals}</TelemetryCell>
          </TelemetryStrip>
          <Caption>
            Every figure is counted from the persisted world or this session's channel. Nothing here
            is a running total the app keeps on its own.
          </Caption>
        </Card>
      </Column>

      <Column>
        <Card
          title={`access · ${selectedSustain()?.summary.label ?? "—"}`}
          right={
            <Show when={access()}>
              {(a) => (
                <Badge tone={a().enforced ? "ok" : "absent"}>
                  {a().enforced ? "enforced" : "not enforced"}
                </Badge>
              )}
            </Show>
          }
        >
          <Show when={access()} fallback={<Empty>reading the capability model…</Empty>}>
            {(a) => (
              <>
                <Readout label="effective privilege">
                  {tierName(a().tier)}
                  {a().tier !== null ? ` · tier ${a().tier}` : ""}
                </Readout>
                <Readout label="membership edges">{a().memberships}</Readout>
                <Caption>
                  Lower is more privileged: 0 owner, 1 member, 2 contributor, 3 observer. The
                  effective tier is the <strong>weakest link</strong> along the membership path,
                  computed by <code>effective_privilege</code> — not read off a single edge.
                </Caption>

                <Note gap="lg">
                  <Label>what this principal may run</Label>
                </Note>
                <For each={a().operators} fallback={<Empty>no operators declared</Empty>}>
                  {(o) => (
                    <Row>
                      <Value>{o.operator}</Value>
                      <Meta>needs {tierName(o.requiredTier)}</Meta>
                      <Spacer />
                      <Value tone={o.permitted ? "ok" : "danger"}>
                        {o.permitted ? "permitted" : (o.denial ?? "denied")}
                      </Value>
                    </Row>
                  )}
                </For>

                <Note gap="lg">
                  <Absent title="displayed, not enforced">{a().note}</Absent>
                </Note>
              </>
            )}
          </Show>
        </Card>
      </Column>
    </Split>
  );
}
