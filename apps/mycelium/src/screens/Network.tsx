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
import { engine, type PeerDto, type SyncDto } from "../lib/engine";

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
  const [busy, setBusy] = createSignal<string | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);
  const [last, setLast] = createSignal<SyncDto | null>(null);

  const run = async (what: string, f: () => Promise<unknown>) => {
    setBusy(what);
    setFailure(null);
    try {
      await f();
      await refetch();
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
                      This node is locked, so it cannot prove its own key — and a peer that
                      cannot be answered is a peer that cannot sync. Unlock to rejoin the
                      network.
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
                        Binds a port on this machine. LAN and localhost only — there is no
                        discovery and no NAT traversal, so a peer needs an address you give
                        it.
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

            <AddPeer busy={busy()} onRun={run} />

            <Show when={last()}>{(r) => <SyncResult report={r()} />}</Show>

            <Show when={failure()}>
              {(f) => (
                <Card title="that did not work">
                  <ErrorState>{f()}</ErrorState>
                </Card>
              )}
            </Show>

            {/* ── what this is not ───────────────────────────────────────── */}
            <Card title="what this transport is not">
              <Caption>
                Two nodes on localhost or a LAN, connected by hand and authenticated by
                ed25519 key. There is <strong>no discovery</strong> (a peer is added by
                address), <strong>no NAT traversal</strong> (both ends must be able to
                reach each other), <strong>no transport encryption</strong> (the handshake
                proves identity; it does not hide the bytes, which is honest for a LAN and
                not enough for the open internet), and <strong>no gossip</strong> — sync is
                pairwise and pull-based. Each of those is a named gap rather than a
                fabricated feature.
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
        <Meta>{props.peer.address ?? "no address — it reached in, this node cannot dial out"}</Meta>
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
        A peer is added by <strong>key and address</strong> — there is no discovery. Adding
        one records it; it is trusted only when you say so, and shared with only per
        Sustain.
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

function SyncResult(props: { report: SyncDto }) {
  const r = () => props.report;
  return (
    <Card title="last sync" right={<Meta>{r().sustainId}</Meta>}>
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
          Every entry survives — the log is a grow-only set and nothing was dropped. What
          did not survive is a <strong>value</strong>: both nodes wrote the same path
          without having seen each other, and the fold order picked one. A number that must
          never be overwritten needs a counter that merges by maximum, not a plain field.
        </Caption>
      </Show>

      <Show when={r().concurrent > 0}>
        <Caption>
          {r().concurrent} pair(s) were genuinely concurrent — neither happened before the
          other, and the order they folded in was a tie-break rather than a fact.
        </Caption>
      </Show>

      <Show when={r().forks > 0}>
        <Caption>
          {r().forks} stamp(s) arrived carrying two different payloads — a node that forked
          its own history. The winner is deterministic on every replica; the fact is
          recorded rather than resolved away.
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
            Every entry was admitted by the node that made it, against the state that node
            could see. The merge was admitted by nobody. Nothing has been repaired — which
            of two admitted facts to give up is a decision, not an arithmetic.
          </Caption>
        </Note>
      </Show>

      <Show when={r().admissible === null}>
        <Caption>
          admissibility unmeasured — this Sustain has no armed enforcement, so there was
          nothing to check the merged state against. That is not the same as passing.
        </Caption>
      </Show>
    </Card>
  );
}
