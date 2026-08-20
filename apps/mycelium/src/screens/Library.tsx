/**
 * **LIBRARY** — the arena, where an artifact becomes a package and back again.
 *
 * ★★★ **The three questions stay three all the way to the screen.** *Are these
 * the published bytes* (integrity), *did that key publish them* (authenticity)
 * and *is it any good* (provenance) are shown as three separate facts, because
 * a single "verified ✓" badge is how a registry teaches people to stop
 * looking. A package from a key you would trust with anything still shows its
 * gate verdict beside it.
 *
 * ★★★ **The verdict is recomputed, never remembered.** What is shown is what
 * the gate says *now* — a definition that typechecked at publish can be
 * refused today because a live instance moved since.
 *
 * ★★ **Royalties are internal juul.** The boundary is on screen, in the same
 * words the Economy panel uses: never money, never a rail, never off this host.
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
import {
  engine,
  type InstallDto,
  type PackageDto,
  type PeerShelfDto,
  type RoyaltyDto,
} from "../lib/engine";

const KINDS = ["definition", "widget", "operator", "strategy"] as const;

const short = (h: string) => (h.length > 16 ? `${h.slice(0, 10)}…` : h);

export function Library() {
  const [target, setTarget] = createSignal<string | null>(null);
  const [lib, { refetch }] = createResource(target, (t) => engine.library(t));
  // ★ Not fetched on mount: listing a peer opens a real socket to it, and a
  //   screen should not go knocking on the network because it was rendered.
  const [shelves, setShelves] = createSignal<PeerShelfDto[] | null>(null);
  const [busy, setBusy] = createSignal<string | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);
  const [last, setLast] = createSignal<InstallDto | null>(null);
  const [paid, setPaid] = createSignal<RoyaltyDto | null>(null);

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
      <Show when={lib.error}>
        <ErrorState>{String(lib.error)}</ErrorState>
      </Show>

      <Show when={lib()}>
        {(l) => (
          <>
            {/* ── the shelf ──────────────────────────────────────────────── */}
            <Card
              title="packages"
              right={<Meta>{`${l().packages.length}`}</Meta>}
            >
              <Cluster>
                <Label>installing into</Label>
                {/* ★ A widget is judged against the household it would join, so
                    the target is part of the question, not a detail. */}
                <For each={l().targets}>
                  {([id, label]) => (
                    <Chip onClick={() => setTarget(target() === id ? null : id)}>
                      {target() === id ? `· ${label}` : label}
                    </Chip>
                  )}
                </For>
              </Cluster>
              <Show when={!target()}>
                <Caption>
                  No household chosen — a definition installs node-wide and does not need
                  one, but a widget will be refused until you pick where it goes.
                </Caption>
              </Show>

              <Show
                when={l().packages.length > 0}
                fallback={
                  <Empty>
                    nothing published yet · an artifact becomes a package when it is
                    stamped, hashed and passes its own typecheck
                  </Empty>
                }
              >
                <For each={l().packages}>
                  {(p) => (
                    <PackageRow
                      pkg={p}
                      target={target()}
                      busy={busy()}
                      onRun={run}
                      onInstalled={setLast}
                      onPaid={setPaid}
                    />
                  )}
                </For>
              </Show>
            </Card>

            {/* ── from peers ───────────────────────────────────── */}
            <Card
              title="from peers"
              right={
                <Chip
                  onClick={() =>
                    void run("shelves", async () => setShelves(await engine.shelves()))
                  }
                >
                  {busy() === "shelves" ? "asking…" : "look"}
                </Chip>
              }
            >
              <Caption>
                Only the peers you added and trusted — there is no index, no mesh search
                and no global catalogue. Looking opens a real connection to each of them
                over the encrypted session; nothing is transferred by looking.
              </Caption>

              <Show when={shelves()}>
                {(list) => (
                  <Show
                    when={list().length > 0}
                    fallback={
                      <Empty>
                        no trusted peers with an address · add one on the Network screen
                      </Empty>
                    }
                  >
                    <For each={list()}>
                      {(shelf) => (
                        <Note>
                          <Row>
                            <Value>{shelf.handle}</Value>
                            <Meta>{short(shelf.peer)}</Meta>
                            <Spacer />
                            <Meta>{shelf.address}</Meta>
                          </Row>
                          {/* ★★★ Unreachable is said, not shown as an empty shelf. */}
                          <Show
                            when={!shelf.unreachable}
                            fallback={<Caption>could not be reached · {shelf.unreachable}</Caption>}
                          >
                            <Show
                              when={shelf.packages.length > 0}
                              fallback={<Caption>reachable, and offering nothing</Caption>}
                            >
                              <For each={shelf.packages}>
                                {(o) => (
                                  <Row>
                                    <Value>{o.name}</Value>
                                    <Meta>{o.kind}</Meta>
                                    <Badge tone={o.signed ? "ok" : "quiet"}>
                                      {o.signed ? "signed" : "unsigned"}
                                    </Badge>
                                    <Spacer />
                                    <Show
                                      when={!o.alreadyHere}
                                      fallback={<Meta>already here</Meta>}
                                    >
                                      <Chip
                                        onClick={() =>
                                          void run(`fetch-${o.contentHash}`, async () => {
                                            await engine.fetchPackage(
                                              shelf.address,
                                              o.contentHash,
                                            );
                                            setShelves(await engine.shelves());
                                          })
                                        }
                                      >
                                        {busy() === `fetch-${o.contentHash}` ? "fetching…" : "fetch"}
                                      </Chip>
                                    </Show>
                                  </Row>
                                )}
                              </For>
                              <Caption>
                                Fetching copies it here. It is <strong>not installed</strong> by
                                arriving — it joins the shelf above and faces exactly the gate a
                                package written on this machine faces.
                              </Caption>
                            </Show>
                          </Show>
                        </Note>
                      )}
                    </For>
                  </Show>
                )}
              </Show>
            </Card>

            <Show when={last()}>{(r) => <Outcome result={r()} />}</Show>
            <Show when={paid()}>{(r) => <Royalty result={r()} />}</Show>

            {/* ── publish ────────────────────────────────────────────────── */}
            <Show
              when={l().unlocked}
              fallback={
                <Card title="publish">
                  <Caption>
                    This node is locked, so nothing can be published under its key. A
                    package without an author is not a package.
                  </Caption>
                </Card>
              }
            >
              <Publish
                publishable={l().publishable}
                target={target()}
                busy={busy()}
                onRun={run}
                onDone={setLast}
              />
            </Show>

            <Show when={failure()}>
              {(f) => (
                <Card title="that did not work">
                  <ErrorState>{f()}</ErrorState>
                </Card>
              )}
            </Show>

            {/* ── the boundary ───────────────────────────────────────────── */}
            <Card title="what a package trades in">
              <Caption>
                Definitions, widgets, operator names, and <strong>trust</strong> — priced,
                where they are priced at all, in <strong>juul</strong>: this host's own
                internal accounting unit. Juul is <strong>never real money</strong>, never
                transferable off this host, and never a payment rail. A royalty is a
                transfer between balances on this machine, so total circulation is
                unchanged by construction — nothing is minted and nothing leaves.
              </Caption>
              <Caption>
                <strong>Pull, from peers you chose.</strong> A package travels when you
                ask for it by content hash — a peer never sends an artifact unsolicited,
                because an unsolicited-artifact channel is an unsolicited-code channel.
                What is <strong>not</strong> here: discovery of packages across a mesh, a
                trust score with anything behind it, and orders.
              </Caption>
            </Card>
          </>
        )}
      </Show>
    </Column>
  );
}

/* ── one package ──────────────────────────────────────────────────────────── */

function PackageRow(props: {
  pkg: PackageDto;
  target: string | null;
  busy: string | null;
  onRun: (what: string, f: () => Promise<unknown>) => Promise<void>;
  onInstalled: (r: InstallDto) => void;
  onPaid: (r: RoyaltyDto) => void;
}) {
  const [open, setOpen] = createSignal(false);
  const p = () => props.pkg;

  return (
    <Note>
      <Row>
        <Value>{p().name}</Value>
        <Meta>{p().kind}</Meta>
        <Meta>v{p().version}</Meta>
        <Spacer />
        <Show when={p().installed}>
          <Badge tone="ok">installed</Badge>
        </Show>
        <Chip onClick={() => setOpen((o) => !o)}>{open() ? "less" : "more"}</Chip>
      </Row>

      {/* ★★★ Three facts, three badges. Never one. */}
      <Row>
        <Badge tone={p().integrity === "intact" ? "ok" : "danger"}>
          {p().integrity === "intact" ? "bytes intact" : "BYTES ALTERED"}
        </Badge>
        <Badge
          tone={
            p().authenticity === "signed"
              ? "ok"
              : p().authenticity === "forged"
                ? "danger"
                : "quiet"
          }
        >
          {p().authenticity}
        </Badge>
        <Meta>{p().origin}</Meta>
        <Spacer />
        <Meta>{p().perMille > 0 ? `${p().perMille}‰ juul` : "free"}</Meta>
      </Row>

      <Show when={p().description}>
        <Caption>{p().description}</Caption>
      </Show>

      {/* ★★ The gate's answer, recomputed on every read. */}
      <Caption>{p().verdict}</Caption>

      <Show when={open()}>
        <Note gap="sm">
          <Split>
            <Readout label="author key">{short(p().author)}</Readout>
            <Readout label="handle">{p().authorHandle || "—"}</Readout>
            <Readout label="content hash">{short(p().contentHash)}</Readout>
          </Split>
          <Caption>
            The <strong>key</strong> is the author; the handle beside it is a label they
            chose and proves nothing on its own.
          </Caption>
          <Show when={p().tags.length > 0}>
            <Cluster>
              <For each={p().tags}>{(t) => <Meta>{t}</Meta>}</For>
            </Cluster>
          </Show>
          <Show when={p().installedInto}>
            {(into) => <Caption>installed into {into()}</Caption>}
          </Show>

          <Cluster>
            <Button
              disabled={!p().installable || props.busy === `install-${p().id}`}
              onClick={() =>
                void props.onRun(`install-${p().id}`, async () => {
                  const r = await engine.install(p().id, props.target);
                  props.onInstalled(r);
                })
              }
            >
              {props.busy === `install-${p().id}` ? "asking the gate…" : "install"}
            </Button>
            <Show when={p().perMille > 0}>
              <Chip
                onClick={() =>
                  void props.onRun(`pay-${p().id}`, async () => {
                    const r = await engine.payRoyalty(p().id, 1000);
                    props.onPaid(r);
                  })
                }
              >
                pay its royalty on 1,000 juul
              </Chip>
            </Show>
          </Cluster>
          <Show when={!p().installable}>
            <Caption>
              Install is unavailable because the gate says so above — not because of who
              published it. Trust decides what you choose; it never decides what is
              allowed.
            </Caption>
          </Show>
        </Note>
      </Show>
    </Note>
  );
}

/* ── publishing ───────────────────────────────────────────────────────────── */

function Publish(props: {
  publishable: [string, string][];
  target: string | null;
  busy: string | null;
  onRun: (what: string, f: () => Promise<unknown>) => Promise<void>;
  onDone: (r: InstallDto) => void;
}) {
  const [kind, setKind] = createSignal<string>("definition");
  const [pick, setPick] = createSignal<string | null>(null);
  const [name, setName] = createSignal("");
  const [description, setDescription] = createSignal("");
  const [perMille, setPerMille] = createSignal("0");
  const [spec, setSpec] = createSignal("");

  const publish = async () => {
    let body: unknown;
    if (kind() === "definition") {
      const id = pick();
      if (!id) throw new Error("pick a definition");
      // ★ The package IS the authored definition, byte for byte — there is no
      //   separate publishable form, so nothing can drift between what was
      //   authored and what was shipped.
      const all = await engine.definitions();
      const found = all.find((d) => d.id === id);
      if (!found) throw new Error(`no definition called ${id}`);
      body = found;
    } else {
      body = JSON.parse(spec());
    }
    const r = await engine.publish({
      name: name() || pick() || "untitled",
      kind: kind(),
      version: "1.0.0",
      description: description(),
      tags: [],
      spec: body as never,
      perMille: Number(perMille()) || 0,
      into: props.target,
    });
    props.onDone(r);
  };

  return (
    <Card title="publish">
      <Cluster>
        <Label>kind</Label>
        <For each={KINDS}>
          {(k) => (
            <Chip onClick={() => setKind(k)}>{kind() === k ? `· ${k}` : k}</Chip>
          )}
        </For>
      </Cluster>

      <Show
        when={kind() === "definition"}
        fallback={
          <Field label="spec (JSON)">
            <textarea
              class={S.input}
              rows={6}
              value={spec()}
              placeholder='{"id":"card","render":"readout","inputs":["liquid"],"emits":[]}'
              onInput={(e) => setSpec(e.currentTarget.value)}
            />
          </Field>
        }
      >
        <Cluster>
          <Label>which definition</Label>
          <Show
            when={props.publishable.length > 0}
            fallback={<Caption>every definition authored here is already published</Caption>}
          >
            <For each={props.publishable}>
              {([id, label]) => (
                <Chip onClick={() => setPick(id)}>{pick() === id ? `· ${label}` : label}</Chip>
              )}
            </For>
          </Show>
        </Cluster>
      </Show>

      <Field label="name">
        <input class={S.input} value={name()} onInput={(e) => setName(e.currentTarget.value)} />
      </Field>
      <Field label="description">
        <input
          class={S.input}
          value={description()}
          onInput={(e) => setDescription(e.currentTarget.value)}
        />
      </Field>
      <Field label="royalty · juul per mille">
        <input
          class={S.input}
          value={perMille()}
          onInput={(e) => setPerMille(e.currentTarget.value)}
        />
      </Field>

      <Button
        disabled={props.busy === "publish"}
        onClick={() => void props.onRun("publish", publish)}
      >
        {props.busy === "publish" ? "typechecking…" : "publish"}
      </Button>
      <Caption>
        Publishing runs the artifact's real typecheck first. One that does not pass is{" "}
        <strong>refused and not stored</strong> — a registry that kept broken artifacts
        would just be a place where they wait.
      </Caption>
    </Card>
  );
}

/* ── outcomes ─────────────────────────────────────────────────────────────── */

function Outcome(props: { result: InstallDto }) {
  const r = () => props.result;
  return (
    <Card title="last attempt">
      <Row>
        <Badge
          tone={r().outcome === "admitted" ? "ok" : r().outcome === "refused" ? "danger" : "quiet"}
        >
          {r().outcome.replace("_", " ")}
        </Badge>
        <Spacer />
        <Show when={r().rule}>{(rule) => <Meta>[{rule()}]</Meta>}</Show>
      </Row>
      <Caption>{r().summary}</Caption>
      <Show when={r().errors.length > 0}>
        <Label>in the gate's own words</Label>
        <For each={r().errors}>{(e) => <Caption>{e}</Caption>}</For>
      </Show>
      <Show when={r().applied}>
        {(id) => <Caption>applied as {id()} — it is now an artifact of this node like any other</Caption>}
      </Show>
      <Caption>provenance · {r().provenance}</Caption>
    </Card>
  );
}

function Royalty(props: { result: RoyaltyDto }) {
  const r = () => props.result;
  return (
    <Card title="royalty · in juul">
      <Split>
        <Readout label="moved">{String(r().transferred)}</Readout>
        <Readout label="circulation before">{r().circulationBefore}</Readout>
        <Readout label="circulation after">{r().circulationAfter}</Readout>
      </Split>
      <Show when={r().shares.length > 0}>
        <For each={r().shares}>
          {([role, to, amount]) => (
            <Row>
              <Value>{role}</Value>
              <Meta>{to}</Meta>
              <Spacer />
              <Meta>{String(amount)}</Meta>
            </Row>
          )}
        </For>
      </Show>
      <Caption>
        {/* ★★★ Asserted on screen, not just in a test. */}
        Circulation is unchanged: a royalty is a <strong>transfer</strong> between balances
        on this host. Nothing was minted, nothing left, and none of it is money. A share
        owed to the payer stays where it is and is still reported.
      </Caption>
      <Show when={r().outcome === "no_royalty"}>
        <Caption>this package is free — free of royalty, never free of the work it causes</Caption>
      </Show>
    </Card>
  );
}
