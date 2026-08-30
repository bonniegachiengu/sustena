/**
 * **NETWORK** — this node, and the peers it has actually decided about.
 *
 * ★★★ Where an honest absence used to be. The `— single node · no transport`
 * panel is gone because it is no longer true; what replaced it is not a peer
 * count but a **decision list**, because that is what a peer relationship is
 * here: a key that authenticated, and a person who did or did not trust it.
 *
 * ★★★ **Two grants, shown as two.** Trusting a key and sharing a Sustain are
 * separate everywhere in the engine, so they are separate here — a trusted
 * peer with no shares reads as exactly that, rather than as access.
 *
 * ★★ **A sync reports what the merge could not decide.** Converged is not the
 * end of the sentence: concurrent writes to one path are named with which
 * value won, and a merged state that no longer satisfies the household's own
 * rules is said out loud. Every entry survives; a value may not.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { world } from "../lib/live";
import { openRow } from "../lib/nav";
import {
  Badge,
  Button,
  Caption,
  Card,
  Chip,
  Cluster,
  Column,
  Empty,
  ErrorState,
  Field,
  Label,
  Meta,
  Note,
  Readout,
  Row,
  Spacer,
  Split,
  Value,
  sx as S,
} from "../ui";
import { engine, type BodyDto, type PeerDto, type SyncDto } from "../lib/engine";

/** A key is 64 hex characters. A person needs the ends, not the middle. */
const short = (key: string) => (key.length > 16 ? `${key.slice(0, 8)}…${key.slice(-4)}` : key);

const when = (secs: string | null) => {
  if (!secs) return null;
  const n = Number(secs);
  if (!Number.isFinite(n)) return null;
  return new Date(n * 1000).toLocaleString();
};

export function Network() {
  const [net, { refetch }] = createResource(() => engine.network());
  const [bodies, { refetch: refetchBodies }] = createResource(() => engine.bodies());
  const [busy, setBusy] = createSignal<string | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);
  const [last, setLast] = createSignal<SyncDto | null>(null);

  const run = async (what: string, f: () => Promise<unknown>) => {
    setBusy(what);
    setFailure(null);
    try {
      await f();
      await refetch();
      await refetchBodies();
    } catch (e) {
      setFailure(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Column>
      <Show when={net.error}>
        <ErrorState>{String(net.error)}</ErrorState>
      </Show>

      <Show when={net()}>
        {(n) => (
          <>
            {/* ── this node ──────────────────────────────────────────────── */}
            <Card title="this node">
              <Split>
                <Readout label="handle">{n().handle ?? "—"}</Readout>
                <Readout label="key">
                  {n().nodeId ? short(n().nodeId!) : "—"}
                </Readout>
                <Readout label="listening on">
                  {/* ★ `None` is "not listening", never a port called 0. */}
                  {n().listening !== null ? String(n().listening) : "not listening"}
                </Readout>
              </Split>

              <Show
                when={n().unlocked}
                fallback={
                  <Note>
                    <Caption>
                      This node is locked, so it cannot prove its key or answer a peer.
                      Unlock to rejoin the network.
                    </Caption>
                  </Note>
                }
              >
                <Show when={n().listening === null}>
                  <Note>
                    <Cluster>
                      <Button
                        onClick={() => void run("listen", () => engine.listen(null))}
                        disabled={busy() === "listen"}
                      >
                        {busy() === "listen" ? "binding…" : "start listening"}
                      </Button>
                      <Caption>
                        Binds a port on this machine, for your local network only. A peer
                        needs an address you give it.
                      </Caption>
                    </Cluster>
                  </Note>
                </Show>
              </Show>

              <Show when={n().nodeId}>
                {(id) => (
                  <Note>
                    <Label>the whole key, for a peer to add</Label>
                    <div class={S.meta}>{id()}</div>
                  </Note>
                )}
              </Show>

              {/* ★★★ What a session actually provides, named — not a padlock
                  that is green whatever happened. */}
              <Note>
                <Row>
                  <Label>every session</Label>
                  <Spacer />
                  <Badge tone="ok">encrypted · wire v{n().protocol}</Badge>
                </Row>
                <Caption>{n().session}</Caption>
                <Caption>
                  Each connection agrees a fresh key, and both sides sign the exchange
                  with their identity keys, so the key is tied to who you are talking to.
                  The temporary secret is never written down.
                </Caption>
                <Caption>
                  <strong>There is no unencrypted mode.</strong> A peer speaking the older
                  plain protocol is refused before any keys are exchanged.
                </Caption>
                <Caption>
                  Covered: confidentiality, tamper detection, forward secrecy, and
                  authenticated peers. Still missing: post-quantum key exchange, a formal
                  proof, and rekeying on a long session.
                </Caption>
              </Note>
            </Card>

            {/* ── the peers ──────────────────────────────────────────────── */}
            <Card title="peers" right={<Meta>{`${n().peers.length}`}</Meta>}>
              <Show
                when={n().peers.length > 0}
                fallback={
                  <Empty>
                    no peers yet · a key that connects lands here as pending, and stays
                    pending until you say otherwise
                  </Empty>
                }
              >
                <For each={n().peers}>
                  {(p) => (
                    <PeerRow
                      peer={p}
                      shareable={n().shareable}
                      busy={busy()}
                      onRun={run}
                      onSynced={setLast}
                    />
                  )}
                </For>
              </Show>
            </Card>

            {/* ── shared Sustains ────────────────────────────────────────── */}
            <Show when={(bodies() ?? []).length > 0}>
              <Card title="shared sustains · quorum">
                <Caption>
                  A Sustain with co-owners agrees <strong>before</strong> it writes, so two
                  people writing at once get ordered and neither is overwritten. Everything
                  else on this node writes locally.
                </Caption>
                <For each={bodies() ?? []}>{(b) => <BodyRow body={b} />}</For>
              </Card>
            </Show>

            <AddPeer busy={busy()} onRun={run} />

            <Show when={last()}>{(r) => <SyncResult report={r()} />}</Show>

            <Show when={failure()}>
              {(f) => (
                <Card title="failed">
                  <ErrorState>{f()}</ErrorState>
                </Card>
              )}
            </Show>

            {/* ── what this is not ───────────────────────────────────────── */}
            <Card title="limits">
              <Caption>
                Two nodes on the same local network, connected by hand. Still missing:
                automatic discovery, NAT traversal, and gossip between more than two nodes.
                Sync happens one pair at a time, and you start it.
              </Caption>
            </Card>
          </>
        )}
      </Show>
    </Column>
  );
}

/* ── one peer ─────────────────────────────────────────────────────────────── */

function PeerRow(props: {
  peer: PeerDto;
  shareable: [string, string][];
  busy: string | null;
  onRun: (what: string, f: () => Promise<unknown>) => Promise<void>;
  onSynced: (r: SyncDto) => void;
}) {
  const [open, setOpen] = createSignal(false);
  const key = () => props.peer.publicKey;
  const trusted = () => props.peer.standing === "trusted";

  return (
    <Note>
      <Row>
        <Value>{props.peer.handle}</Value>
        <Meta>{short(key())}</Meta>
        <Spacer />
        <Badge tone={trusted() ? "ok" : props.peer.standing === "blocked" ? "danger" : "warn"}>
          {props.peer.standing}
        </Badge>
        <Chip onClick={() => setOpen((o) => !o)}>{open() ? "less" : "more"}</Chip>
      </Row>

      <Row>
        <Meta>{props.peer.address ?? "no address · it connected to you"}</Meta>
        <Spacer />
        <Meta>
          {/* ★ never synced is not "0 minutes ago" */}
          {when(props.peer.lastSynced) ? `last synced ${when(props.peer.lastSynced)}` : "never synced"}
        </Meta>
      </Row>

      <Show when={props.peer.lastError}>
        {(e) => <Caption>last error · {e()}</Caption>}
      </Show>

      <Show when={open()}>
        <Note gap="sm">
          {/* trust */}
          <Cluster>
            <Label>standing</Label>
            <For each={["pending", "trusted", "blocked"]}>
              {(s) => (
                <Chip
                  onClick={() =>
                    void props.onRun(`standing-${key()}`, () => engine.setStanding(key(), s))
                  }
                >
                  {s === props.peer.standing ? `· ${s}` : s}
                </Chip>
              )}
            </For>
          </Cluster>
          <Caption>
            Trusting a key does not share anything with it. Each Sustain is granted on its
            own, below.
          </Caption>

          {/* shares */}
          <Show
            when={trusted()}
            fallback={<Caption>nothing can be shared with a peer that is not trusted</Caption>}
          >
            <Label>shared with this peer</Label>
            <Cluster>
              <For each={props.shareable}>
                {([id, label]) => {
                  const on = () => props.peer.shares.includes(id);
                  return (
                    <Chip
                      onClick={() =>
                        void props.onRun(`share-${id}`, () =>
                          on() ? engine.unshareSustain(key(), id) : engine.shareSustain(key(), id),
                        )
                      }
                    >
                      {on() ? `· ${label}` : label}
                    </Chip>
                  );
                }}
              </For>
            </Cluster>
            <Show when={props.peer.shares.length === 0}>
              <Caption>trusted, and holding nothing of this household</Caption>
            </Show>
          </Show>

          {/* sync */}
          <Show when={trusted() && props.peer.address && props.peer.shares.length > 0}>
            <Cluster>
              <For each={props.peer.shares}>
                {(id) => (
                  <Button
                    disabled={props.busy === `sync-${id}`}
                    onClick={() =>
                      void props.onRun(`sync-${id}`, async () => {
                        const r = await engine.syncWith(props.peer.address!, id);
                        props.onSynced(r);
                      })
                    }
                  >
                    {props.busy === `sync-${id}` ? "syncing…" : `sync ${id}`}
                  </Button>
                )}
              </For>
            </Cluster>
          </Show>
        </Note>
      </Show>
    </Note>
  );
}

/* ── one shared Sustain ──────────────────────────────────────── */

function BodyRow(props: { body: BodyDto }) {
  const b = () => props.body;
  return (
    <Note>
      <Row>
        <Value>{b().label}</Value>
        <Spacer />
        <Badge tone={b().canWrite ? "ok" : "danger"}>
          {b().reachable} of {b().quorum} needed
        </Badge>
      </Row>

      <For each={b().owners}>
        {(o) => (
          <Row>
            <Meta>{o.isSelf ? "this node" : o.handle}</Meta>
            <Meta>{short(o.key)}</Meta>
            <Spacer />
            <Meta>{o.reachable ? "reachable" : "no address"}</Meta>
          </Row>
        )}
      </For>

      {/* ★★★ The honest state, not a silent degradation. */}
      <Show
        when={b().canWrite}
        fallback={
          <Caption>
            <strong>Writes to this Sustain are refused right now.</strong> Too few
            co-owners are reachable. Writing comes back as soon as enough of them are.
          </Caption>
        }
      >
        <Caption>
          Enough co-owners are reachable to try. A peer can still be gone by the time it
          is asked, and the refusal will say so.
        </Caption>
      </Show>

      <Row>
        <Meta>
          {/* ★ A two-node body tolerates ZERO traitors, and a person should know. */}
          tolerates {b().toleratesTraitors ?? 0} traitor
          {(b().toleratesTraitors ?? 0) === 1 ? "" : "s"}
        </Meta>
        <Spacer />
        <Meta>
          {b().lastAgreed !== null ? `last agreed slot ${b().lastAgreed}` : "nothing agreed yet"}
        </Meta>
      </Row>
    </Note>
  );
}

/* ── adding one ───────────────────────────────────────────────────────────── */

function AddPeer(props: {
  busy: string | null;
  onRun: (what: string, f: () => Promise<unknown>) => Promise<void>;
}) {
  const [key, setKey] = createSignal("");
  const [handle, setHandle] = createSignal("");
  const [address, setAddress] = createSignal("");

  return (
    <Card title="add a peer">
      <Caption>
        Add a peer by key and address. Adding records it. You choose when to trust it,
        and what to share, one Sustain at a time.
      </Caption>
      <Field label="public key">
        <input
          class={S.input}
          value={key()}
          placeholder="64 hex characters"
          onInput={(e) => setKey(e.currentTarget.value.trim())}
        />
      </Field>
      <Field label="handle">
        <input
          class={S.input}
          value={handle()}
          placeholder="what they call themselves"
          onInput={(e) => setHandle(e.currentTarget.value)}
        />
      </Field>
      <Field label="address">
        <input
          class={S.input}
          value={address()}
          placeholder="127.0.0.1:9000"
          onInput={(e) => setAddress(e.currentTarget.value.trim())}
        />
      </Field>
      <Button
        disabled={!key() || !address() || props.busy === "add"}
        onClick={() =>
          void props.onRun("add", async () => {
            await engine.addPeer(key(), handle() || "peer", address());
            setKey("");
            setHandle("");
            setAddress("");
          })
        }
      >
        {props.busy === "add" ? "adding…" : "add"}
      </Button>
    </Card>
  );
}

/* ── what the merge could not decide ──────────────────────────────────────── */

/**
 * A Sustain's name in a header, navigable when the store really holds it.
 *
 * ★★★ RULE 2's honest half in one component: `openRow` returns `undefined`
 * when there is nowhere to go, and that same value decides whether this paints
 * as a button or as plain metadata. One value, both decisions — so it cannot
 * end up looking live and doing nothing.
 */
function NavMeta(props: { id: string }) {
  const go = () => openRow(props.id);
  const label = () => world.sustains[props.id]?.summary.label ?? props.id;
  return (
    <Show when={go()} fallback={<Meta>{label()}</Meta>}>
      {(fn) => (
        <button class={`${S.metaButton}`} onClick={fn()}>
          {label()}
        </button>
      )}
    </Show>
  );
}

function SyncResult(props: { report: SyncDto }) {
  const r = () => props.report;
  return (
    <Card
      title="last sync"
      right={
        /* ★★ A sync report names the Sustain it synced and could not take you
           to it. `NavMeta` renders a plain label when the store does not hold
           it — a report about a Sustain this node no longer has is a fact, and
           must not look like a door. */
        <NavMeta id={r().sustainId} />
      }
    >
      <Split>
        <Readout label="received">{String(r().received)}</Readout>
        <Readout label="sent">{String(r().sent)}</Readout>
        <Readout label="entries in the log">{String(r().entries)}</Readout>
      </Split>

      <Note>
        <Caption>{r().summary}</Caption>
      </Note>

      {/* ★★★ Converged is not the whole sentence. */}
      <Show when={r().superseded.length > 0}>
        <Label>values a concurrent write overwrote</Label>
        <For each={r().superseded}>
          {(s) => (
            <Row>
              <Value>{s.path}</Value>
              <Spacer />
              <Meta>
                {s.winner} won · {s.loser} did not
              </Meta>
            </Row>
          )}
        </For>
        <Caption>
          Every entry survives. What was lost is a <strong>value</strong>: both nodes
          wrote the same field without having seen each other, and one had to win.
        </Caption>
      </Show>

      <Show when={r().concurrent > 0}>
        <Caption>
          {r().concurrent} pair(s) happened at the same time. Neither came first, so the
          order was a tie-break.
        </Caption>
      </Show>

      <Show when={r().forks > 0}>
        <Caption>
          {r().forks} stamp(s) arrived with two different payloads, from a node that split
          its own history. Every replica picks the same winner, and this is recorded.
        </Caption>
      </Show>

      <Show when={r().admissible === false}>
        <Note>
          <Label>the merged household is outside its own rules</Label>
          <For each={r().violated}>
            {([id, why]) => (
              <Row>
                <Value>{id}</Value>
                <Spacer />
                <Meta>{why}</Meta>
              </Row>
            )}
          </For>
          <Caption>
            Each entry was accepted by the node that made it, against what that node could
            see. The merged result was checked by nobody, and nothing has been repaired.
          </Caption>
        </Note>
      </Show>

      <Show when={r().admissible === null}>
        <Caption>
          not checked. This Sustain has no rules armed, so there was nothing to check the
          merged state against.
        </Caption>
      </Show>
    </Card>
  );
}
