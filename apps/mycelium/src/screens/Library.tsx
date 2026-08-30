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

import { onPulse } from "../lib/pulse";
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
  Fill,
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
  type OrderDto,
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
  const [orders, { refetch: refetchOrders }] = createResource(() => engine.orders());

  // ★★★ §III: refresh when a change is relayed -- local or arrived by
  //     sync -- so this surface is never showing an answer that has stopped
  //     being true. A package arriving from a peer belongs on the shelf without a reopen.
  onPulse(async () => {
    await refetch();
    await refetchOrders();
  });

  const run = async (what: string, f: () => Promise<unknown>) => {
    setBusy(what);
    setFailure(null);
    try {
      await f();
      await refetch();
      await refetchOrders();
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
                  No household chosen. A definition installs for the whole node. A widget
                  needs one, so pick where it goes first.
                </Caption>
              </Show>

              <Show
                when={l().packages.length > 0}
                fallback={
                  <Empty>
                    nothing published yet · this shelf holds what someone chose to
                    PUBLISH — a definition, widget or operator stamped, hashed and
                    passed through its own typecheck. It is empty because nobody has
                    published one, not because anything is missing. What already runs
                    here is below.
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

            {/* ── what already runs here ───────────────────────── */}
            <BuiltIn target={target()} />

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
                Only the peers you added and trusted. Looking opens a connection to each
                of them. Nothing is transferred by looking.
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
                                Fetching copies it to your shelf. Installing is a separate
                                step, with the same checks as anything written here.
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

            {/* ── what you have acquired ──────────────────────────── */}
            <Show when={(orders() ?? []).length > 0}>
              <Card title="acquired" right={<Meta>{`${(orders() ?? []).length}`}</Meta>}>
                <For each={orders() ?? []}>
                  {(o) => (
                    <Note>
                      <Row>
                        <Value>{o.packageName}</Value>
                        <Meta>{o.reference}</Meta>
                        <Spacer />
                        <Meta>{o.paid > 0 ? `${o.paid} juul` : "free"}</Meta>
                      </Row>
                      <Show when={o.shares.length > 0}>
                        <For each={o.shares}>
                          {([role, to, amount]) => (
                            <Row>
                              <Meta>{role}</Meta>
                              <Meta>{to}</Meta>
                              <Spacer />
                              <Meta>{String(amount)}</Meta>
                            </Row>
                          )}
                        </For>
                      </Show>
                    </Note>
                  )}
                </For>
                <Caption>
                  Acquiring pays the author's share in juul, this app's internal unit.
                  It moves between balances on this machine. <strong>No real money is
                  involved.</strong> Acquiring and installing are separate steps.
                </Caption>
              </Card>
            </Show>

            {/* ── publish ────────────────────────────────────────────────── */}
            <Show
              when={l().unlocked}
              fallback={
                <Card title="publish">
                  <Caption>
                    This node is locked, so nothing can be published under its key. A
                    every package has an author.
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
                <Card title="failed">
                  <ErrorState>{f()}</ErrorState>
                </Card>
              )}
            </Show>

            {/* ── the boundary ───────────────────────────────────────────── */}
            <Card title="what a package trades in">
              <Caption>
                Definitions, widgets, operator names and trust. Where they carry a price
                it is in juul, this app's internal unit. Juul is <strong>never real
                money</strong> and never leaves this machine.
              </Caption>
              <Caption>
                <strong>You pull, from peers you chose.</strong> A package travels only
                when you ask for it. There is no search across a wider network.
              </Caption>
              <Caption>
                <strong>Trust is what you can verify.</strong> Whether a package is
                signed, whether its author is you or a peer you trusted, how many of the
                peers you asked are holding it, and whether you installed it. There are no
                stars and no download counts. <strong>unrated</strong> means nobody has
                said anything about it yet. Trust never decides admission on its own.
              </Caption>
            </Card>
          </>
        )}
      </Show>
    </Column>
  );
}

/* ── one package ──────────────────────────────────────────────────────────── */

/**
 * **What already runs here** — the operators this Sustain may actually call.
 *
 * ★★★ **The screen was empty and the reading was reasonable.** "Library" was
 * built as the ARENA — the shelf of things somebody chose to *publish* — and
 * nobody has published anything, so it showed nothing. But a person looking for
 * *what this system can do* looks in the library, and being told "nothing" when
 * there are dozens of working operators is a wrong answer to a fair question.
 * The two shelves are genuinely different and both belong here: **what runs**
 * and **what was published**.
 *
 * ★★ **It is the Sustain's `T`, not the whole registry** — the same list
 * `get_operators` gives the Console, for the same reason: an operator this
 * definition does not permit would be refused with `operator_allowed`, and
 * listing it would be advertising a refusal a person could not have predicted.
 * So the shelf changes with the subject, which is correct rather than a
 * limitation.
 *
 * ★★★ **Every row opens** (rule 2). An operator names its parameters, its side
 * effects and what it has actually cost — that is real detail, and a row that
 * shows a name and hides the rest is the dead end this pass exists to close.
 */
function BuiltIn(props: { target: string | null }) {
  const [ops] = createResource(
    () => props.target,
    (id) => engine.operators(id),
  );
  const [open, setOpen] = createSignal<string | null>(null);
  const toggle = (name: string) => setOpen(open() === name ? null : name);

  return (
    <Card
      title="what already runs here"
      right={<Meta>{ops()?.length ?? 0}</Meta>}
    >
      <Caption>
        The operators this household may call. They ship with the engine — nothing
        was published to get them, and nothing can remove them but the definition
        that permits them.
      </Caption>

      <Show
        when={props.target}
        fallback={<Empty>no household chosen · pick one to see what it may run</Empty>}
      >
        <Show
          when={(ops() ?? []).length > 0}
          fallback={<Empty>this definition permits no operators</Empty>}
        >
          <For each={ops()}>
            {(o) => (
              <>
                <Row onClick={() => toggle(o.name)}>
                  <Fill>
                    <Value>{o.name}</Value>
                    <Caption>{o.description}</Caption>
                  </Fill>
                  <Spacer />
                  {/* ★★ The measurement or "not measured" — never a zero standing
                      in for a number nobody took. */}
                  <Meta>
                    {o.measured
                      ? `~${o.measured.meanPawa.toFixed(1)} pwa · ${o.measured.runs} run${o.measured.runs === 1 ? "" : "s"}`
                      : "not yet measured"}
                  </Meta>
                </Row>
                <Show when={open() === o.name}>
                  <Note>
                    <Label>parameters</Label>
                    <Show
                      when={o.params.length > 0}
                      fallback={<Caption>none — it takes no arguments</Caption>}
                    >
                      <For each={o.params}>
                        {(p) => (
                          <Row>
                            <Value>{p.name}</Value>
                            <Meta>{p.kind}</Meta>
                            <Spacer />
                            <Meta>{p.required ? "required" : "optional"}</Meta>
                          </Row>
                        )}
                      </For>
                    </Show>

                    <Label>side effects</Label>
                    <Show
                      when={o.sideEffects.length > 0}
                      fallback={<Caption>none declared — it only reads</Caption>}
                    >
                      <Caption>{o.sideEffects.join(" · ")}</Caption>
                    </Show>

                    <Label>cost</Label>
                    <Caption>
                      declared {o.declaredPawa} pwa
                      {o.measured
                        ? ` · measured ${o.measured.totalPawa.toFixed(1)} pwa over ${o.measured.runs} run${o.measured.runs === 1 ? "" : "s"}`
                        : " · never run here, so nothing measured"}
                    </Caption>
                  </Note>
                </Show>
              </>
            )}
          </For>
        </Show>
      </Show>
    </Card>
  );
}

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

      {/* ★★★ The trust reading, and `unrated` is NOT rendered as a score.
          There is no bar, no stars and no number — because nobody produced
          one. */}
      <Row>
        <Show
          when={p().trustIsAReading}
          fallback={<Meta>unrated · nothing is known about this one</Meta>}
        >
          <Badge tone={p().trust === "repudiated" ? "danger" : "quiet"}>{p().trust}</Badge>
        </Show>
        <Spacer />
        <Meta>{p().trustSignals.join(" · ")}</Meta>
      </Row>

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
              {props.busy === `install-${p().id}` ? "running…" : "install"}
            </Button>
            <Chip
              onClick={() =>
                void props.onRun(`order-${p().id}`, async () => {
                  await engine.order(p().id, 1000);
                })
              }
            >
              {props.busy === `order-${p().id}`
                ? "settling…"
                : p().perMille > 0
                  ? `acquire · ${p().perMille}‰ of 1,000 juul`
                  : "acquire · free"}
            </Chip>
            <Show when={p().perMille > 0}>
              <Chip
                onClick={() =>
                  void props.onRun(`pay-${p().id}`, async () => {
                    const r = await engine.payRoyalty(p().id, 1000);
                    props.onPaid(r);
                  })
                }
              >
                royalty only
              </Chip>
            </Show>
          </Cluster>
          <Show when={!p().installable}>
            <Caption>
              Install is unavailable for the reason given above. Who published it makes no
              difference.
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
        Publishing typechecks the artifact first. If it fails, it is <strong>refused and
        not stored</strong>.
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
        {(id) => <Caption>applied as {id()}</Caption>}
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
        <Caption>this package carries no royalty</Caption>
      </Show>
    </Card>
  );
}
