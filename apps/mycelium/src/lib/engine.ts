/**
 * The engine, as the UI sees it.
 *
 * ★★ A thin pass-through over the **generated** bindings — deliberately not a
 * hand-written client. Everything typed here comes from `bindings.ts`, which
 * comes from the Rust in `src-tauri/src/dto.rs`. There is nowhere for the UI
 * and the engine to disagree about a shape.
 */
import {
  commands,
  type AccessDto,
  type AuthoredDefinition,
  type Branch,
  type CouncilOutcomeDto,
  type DefinitionVerdict,
  type DimDecl,
  type InvariantDecl,
  type ConstraintReading,
  type EconomyDto,
  type GateResult,
  type Holarchy,
  type JsonValue,
  type LogEntryDto,
  type OperatorDto,
  type Result,
  type RollupDto,
  type TransferResult,
  type IdentityDto,
  type IngestDto,
  type CaptureResult,
  type ChoiceDto,
  type DeviceDto,
  type NettingDto,
  type OwnIdentifiersDto,
  type FiledSpendDto,
  type SkipLearnedDto,
  type TransferDto,
  type SmsSweep,
  type CaptureContextDto,
  type NetworkDto,
  type PeerDto,
  type SyncDto,
  type LibraryDto,
  type PackageDto,
  type InstallDto,
  type RoyaltyDto,
  type Publication,
  type BodyDto,
  type CoOwnerDto,
  type OfferDto,
  type PeerShelfDto,
  type OrderDto,
  type MessageDto,
  type FeedDto,
  type ShapeOfferDto,
  type TrainedFigureDto,
  type RoutedFigureDto,
  type AutoFiledDto,
  type InferenceDto,
  type AggregateDto,
  type SustainDto,
  type SustainSummary,
  type TemplateId,
  type WorldDto,
  type PersonHint,
} from "../bindings";

export type {
  AccessDto,
  PersonHint,
  AuthoredDefinition,
  Branch,
  CouncilOutcomeDto,
  DefinitionVerdict,
  DimDecl,
  InvariantDecl,
  ConstraintReading,
  EconomyDto,
  OperatorDto,
  GateResult,
  Holarchy,
  JsonValue,
  LogEntryDto,
  RollupDto,
  AggregateDto,
  TransferResult,
  IdentityDto,
  IngestDto,
  CaptureResult,
  ChoiceDto,
  DeviceDto,
  NettingDto,
  OwnIdentifiersDto,
  FiledSpendDto,
  SkipLearnedDto,
  TransferDto,
  SmsSweep,
  CaptureContextDto,
  NetworkDto,
  PeerDto,
  SyncDto,
  LibraryDto,
  PackageDto,
  InstallDto,
  RoyaltyDto,
  Publication,
  BodyDto,
  CoOwnerDto,
  OfferDto,
  PeerShelfDto,
  OrderDto,
  MessageDto,
  FeedDto,
  ShapeOfferDto,
  TrainedFigureDto,
  RoutedFigureDto,
  AutoFiledDto,
  InferenceDto,
  SustainDto,
  SustainSummary,
  TemplateId,
  WorldDto,
};

/**
 * ★ Unwrap a generated `Result`, turning the error half into a thrown value.
 *
 * The generated bindings model a Rust `Result` as a tagged union. That is the
 * right wire shape, but a caller should not have to branch on it at every call
 * site — and crucially the errors these commands return are DISK failures, not
 * gate refusals. A refusal never travels this path: it arrives as a `GateResult`
 * with `verdict: "refused"`, which is a value, not an error.
 */
function unwrap<T>(r: Result<T, string>): T {
  if (r.status === "error") throw new Error(r.error);
  return r.data;
}

export const engine = {
  world: (): Promise<WorldDto> => commands.getWorld(),

  /**
   * ★★★ The local identity. `unlocked` means a private key is in memory —
   * nothing acts until it is, and that is enforced in the HOST, not here.
   * A UI-only gate would be a curtain in front of an open door.
   */
  identity: (): Promise<IdentityDto> => commands.getIdentity(),
  unlock: async (passphrase: string): Promise<IdentityDto> =>
    unwrap(await commands.unlockIdentity(passphrase)),

  /**
   * Come up unlocked from now on, without being asked.
   *
   * ★★★ It caches the DERIVED key, not the passphrase, so a passphrase reused
   * elsewhere is not put at risk -- and only after the passphrase has actually
   * opened the identity, so a wrong one can never be written down as if it
   * were right. `forgetUnlock` deletes it and the gate is exactly as it was.
   */
  rememberUnlock: async (passphrase: string): Promise<void> => {
    unwrap(await commands.rememberUnlock(passphrase));
  },

  forgetUnlock: async (): Promise<void> => {
    unwrap(await commands.forgetUnlock());
  },
  enrol: async (handle: string, passphrase: string): Promise<IdentityDto> =>
    unwrap(await commands.enrolIdentity(handle, passphrase)),
  lockIdentity: (): Promise<IdentityDto> => commands.lockIdentity(),

  /** The capture queue, the declared sources, and the rules in force. */
  ingest: async (sustainId: string): Promise<IngestDto> =>
    unwrap(await commands.getIngest(sustainId)),
  /**
   * ★★★ Capture one message. A message carrying a secret comes back
   * `{kind: "rejected"}` with **no message field at all** — there is nothing to
   * render, because there was nothing to store.
   */
  capture: async (sustainId: string, sourceId: string, raw: string): Promise<CaptureResult> =>
    unwrap(await commands.captureMessage(sustainId, sourceId, raw)),
  // ── reading M-Pesa and KCB texts off the phone ───────────────────────────
  /** "granted" | "denied" | "prompt" | "prompt-with-rationale" */
  /**
   * Where this household's record begins, in unix SECONDS, or null for
   * everything.
   *
   * ★★★ The intake boundary. A message older than this is not captured at all
   * -- on Android it is not even read off the phone -- so starting Sustena
   * today does not open the classify queue with years of texts.
   */
  intakeStart: async (): Promise<number | null> => await commands.getIntakeStart(),

  /**
   * Move where the record begins. ★ Only affects what is captured from now on;
   * it never deletes anything already stored.
   */
  setIntakeStart: async (startAt: number | null): Promise<void> => {
    unwrap(await commands.setIntakeStart(startAt));
  },

  /**
   * What build is actually running — version plus git hash, both stamped at
   * COMPILE time.
   *
   * ★★★ Rendered in the header of both faces. A whole night's work once sat in
   * git while the app on screen was from yesterday and nobody could tell by
   * looking; a hardcoded version would have lied with confidence. This one
   * cannot: it is a property of the binary.
   */
  buildStamp: async (): Promise<string> => await commands.buildStamp(),

  smsPermission: async (): Promise<string> => unwrap(await commands.smsPermissionState()),
  smsRequestPermission: async (): Promise<string> => unwrap(await commands.smsRequestPermission()),
  /**
   * ONE PAGE of the texts already on the phone. 0 days means the whole inbox.
   *
   * Paged because it has to be. A phone holding a few thousand texts froze the
   * app when every match was read and captured in one call. The caller loops
   * while `hasMore` and shows progress between pages.
   */
  smsImportPage: async (
    sustainId: string,
    sinceDays: number,
    offset: number,
    limit: number,
  ): Promise<SmsSweep> =>
    unwrap(await commands.smsImportPage(sustainId, sinceDays, offset, limit)),
  /** ONE BATCH of what arrived while the app was shut. Loop while `hasMore`. */
  smsDrain: async (sustainId: string, limit: number): Promise<SmsSweep> =>
    unwrap(await commands.smsDrainQueue(sustainId, limit)),
  /** How many texts are waiting, without taking any. */
  smsQueueDepth: async (): Promise<number> => unwrap(await commands.smsQueueDepth()),
  /**
   * The text a notification tap was about, taken once.
   *
   * Null on every launch that was not a tap, which is most of them.
   */
  smsPendingClassify: async (): Promise<string | null> =>
    unwrap(await commands.smsPendingClassify()),
  /** Take the prompt down, once the queue has actually been swept. */
  smsClearPrompt: async (): Promise<null> => unwrap(await commands.smsClearPrompt()),
  /**
   * Whether the classify prompt may be posted.
   *
   * The difference between a real-time reader and a batch importer: without
   * it the text is still captured and nobody is told until the app is opened.
   */
  smsNotifyState: async (): Promise<string> => unwrap(await commands.smsNotifyState()),

  declareSource: async (id: string, label: string, minutes: number | null): Promise<null> =>
    unwrap(await commands.declareSource(id, label, minutes)),
  /** The phone and account numbers this household calls its own. */
  ownIdentifiers: async (): Promise<OwnIdentifiersDto> =>
    unwrap(await commands.getOwnIdentifiers()),
  /** Record them. Stays on the device. */
  setOwnIdentifiers: async (own: OwnIdentifiersDto): Promise<OwnIdentifiersDto> =>
    unwrap(await commands.setOwnIdentifiers(own)),
  /** Turn each pair of texts that is really one move into one move. */
  applyTransfers: async (sustainId: string): Promise<TransferDto> =>
    unwrap(await commands.applyTransfers(sustainId)),
  /** Cancel refunds against their charges. Returns what it did. */
  netReversals: async (sustainId: string): Promise<NettingDto> =>
    unwrap(await commands.netReversals(sustainId)),
  /** Move a spend filed to the wrong pocket. Appends a correction. */
  reclassify: async (
    sustainId: string,
    messageId: string,
    fromPocket: string,
    toPocket: string,
    amount: number,
  ): Promise<GateResult> =>
    unwrap(await commands.reclassifySpend(sustainId, messageId, fromPocket, toPocket, amount)),
  /** Does this message carry a number, and is it already somebody's tab? */
  personHint: async (sustainId: string, messageId: string): Promise<PersonHint> =>
    unwrap(await commands.personHint(sustainId, messageId)),
  /** Tie a phone number to a pocket, so money both ways lands in that tab. */
  linkNumber: async (
    sustainId: string,
    pocketName: string,
    number: string,
  ): Promise<GateResult> =>
    unwrap(await commands.linkNumber(sustainId, pocketName, number)),
  /** Put a message off until he remembers. It returns at the top next open. */
  deferMessage: async (sustainId: string, messageId: string): Promise<boolean> =>
    unwrap(await commands.deferMessage(sustainId, messageId)),
  /** "Never ask me about these again" — learn a skip from one message. */
  learnSkip: async (sustainId: string, messageId: string): Promise<SkipLearnedDto> =>
    unwrap(await commands.learnSkip(sustainId, messageId)),
  /** Set a captured message aside as not a transaction. Keeps the record. */
  ignoreMessage: async (id: string): Promise<boolean> =>
    unwrap(await commands.ignoreMessage(id)),
  resolveMessage: async (id: string): Promise<boolean> =>
    unwrap(await commands.resolveMessage(id)),
  // ── Orchie ──────────────────────────────────────────────────────────────
  /** `compose(r)` — the curated feed for one household. Read-only. */
  feed: async (sustainId: string, query: string | null): Promise<FeedDto> =>
    unwrap(await commands.getFeed(sustainId, query)),
  /** ε → (o, θ). Resolves or asks one question; never writes. */
  infer: async (
    sustainId: string,
    messageId: string | null,
    effectText: string | null,
    known: JsonValue,
    ignoreHistory: boolean,
  ): Promise<InferenceDto> =>
    unwrap(await commands.orchieInfer(sustainId, messageId, effectText, known, ignoreHistory)),
  /** Confirm — the operator through the real gate, as the unlocked principal. */
  confirm: async (
    sustainId: string,
    operator: string,
    params: JsonValue,
    messageId: string | null,
    description: string | null,
    /**
     * Whether this call finishes the message off.
     *
     * Filing a past charge takes two calls, and only the second one deals with
     * it. Both are recorded against the message either way, because undoing a
     * charge later needs to know both halves of how it was filed.
     */
    resolves: boolean | null = null,
  ): Promise<GateResult> =>
    unwrap(
      await commands.orchieConfirm(sustainId, operator, params, messageId, description, resolves),
    ),

  /** "remember this format" — synthesise a rule from a confirmed correction. */
  learnRule: async (messageId: string, operator: string, params: JsonValue): Promise<string> =>
    unwrap(await commands.learnRule(messageId, operator, params)),

  /**
   * Teach a shape by pointing at its figures.
   *
   * ★★ Distinct from `learnRule` on purpose. That one learns from a decision
   * already made; this one takes what a person is SAYING about the message in
   * front of them — and only this one can carry more than one figure, which a
   * Fuliza borrow needs and a single confirmed amount cannot express.
   */
  trainRule: async (messageId: string, figures: TrainedFigureDto[]): Promise<string> =>
    unwrap(await commands.trainRule(messageId, figures)),

  /**
   * What filed itself, and what disagrees with it.
   *
   * ★★ A read. The guard makes automatic filings visible; it has no power to
   * change one, because a guard that could act without being asked would be
   * another instance of the thing it exists to catch.
   */
  autoFiled: async (sustainId: string, limit: number | null = null): Promise<AutoFiledDto[]> =>
    unwrap(await commands.autoFiled(sustainId, limit)),
  sustain: (id: string | null = null): Promise<SustainDto | null> => commands.getSustain(id),
  select: async (id: string): Promise<boolean> => unwrap(await commands.selectSustain(id)),
  create: async (
    id: string,
    label: string,
    template: TemplateId,
    parent: string | null,
  ): Promise<boolean> => unwrap(await commands.createSustain(id, label, template, parent)),
  constraints: (id: string): Promise<ConstraintReading[]> => commands.getConstraints(id),
  operators: (id: string): Promise<OperatorDto[]> => commands.getOperators(id),
  economy: (): Promise<EconomyDto> => commands.getEconomy(),
  setParameter: (name: string, value: number): Promise<GateResult> =>
    commands.setParameter(name, value),
  definitions: (): Promise<AuthoredDefinition[]> => commands.getDefinitions(),
  authorDefinition: async (d: AuthoredDefinition): Promise<DefinitionVerdict> =>
    unwrap(await commands.authorDefinition(d)),
  createFromDefinition: async (
    id: string,
    label: string,
    definitionId: string,
    parent: string | null,
  ): Promise<boolean> =>
    unwrap(await commands.createFromDefinition(id, label, definitionId, parent)),
  access: (sustainId: string): Promise<AccessDto> => commands.getAccess(sustainId),
  /**
   * ρ — a Sustain's declared totals, folded fresh by the engine.
   *
   * ★ `null` only when there is no such Sustain. A Sustain that declares no
   * totals answers with an empty `aggregates` list, which is a different fact
   * and must not render the same.
   */
  rollup: (sustainId: string): Promise<RollupDto | null> => commands.getRollup(sustainId),
  /**
   * The atomic, conserved cross-Sustain transfer.
   *
   * ★ A refusal comes back as a VALUE (`kind: "refused"`), not a thrown error.
   * `unwrap` only surfaces a disk failure — the gate declining is an outcome,
   * and putting it in the same bucket as a broken store would make a normal
   * "no" look like a crash.
   */
  transfer: async (
    fromSustainId: string,
    toSustainId: string,
    path: string,
    amount: number,
  ): Promise<TransferResult> =>
    unwrap(await commands.transfer(fromSustainId, toSustainId, path, amount)),
  resolveProposal: (
    votes: [string, string, number][],
    userVote: string | null,
    collected: boolean,
  ): Promise<CouncilOutcomeDto> => commands.resolveProposal(votes, userVote, collected),
  /** A hypothetical branch. Nothing is written. */
  simulate: (id: string, steps: [string, JsonValue][]): Promise<Branch | null> =>
    commands.simulate(id, steps),
  log: async (id: string): Promise<LogEntryDto[]> => unwrap(await commands.getLog(id)),
  /** `null` means there is no such Sustain — a different fact from a refusal. */
  run: async (
    sustainId: string,
    operator: string,
    params: JsonValue,
  ): Promise<GateResult | null> => unwrap(await commands.runOperator(sustainId, operator, params)),

  // ── the network ─────────────────────────────────────────────────
  /** This node, its peers, and whether it is reachable at all. */
  network: async (): Promise<NetworkDto> => await commands.getNetwork(),
  /** Start accepting peers. A locked node refuses. */
  listen: async (port: number | null): Promise<number> =>
    unwrap(await commands.startListening(port)),
  /** Record a key and an address, granting nothing. */
  addPeer: async (publicKey: string, handle: string, address: string): Promise<null> =>
    unwrap(await commands.addPeer(publicKey, handle, address)),
  /** `pending` | `trusted` | `blocked`. Trusting is not sharing. */
  setStanding: async (publicKey: string, standing: string): Promise<boolean> =>
    unwrap(await commands.setPeerStanding(publicKey, standing)),
  shareSustain: async (publicKey: string, sustainId: string): Promise<null> =>
    unwrap(await commands.shareSustain(publicKey, sustainId)),
  unshareSustain: async (publicKey: string, sustainId: string): Promise<boolean> =>
    unwrap(await commands.unshareSustain(publicKey, sustainId)),
  /** Converge one Sustain with one peer, both directions, in one session. */
  syncWith: async (address: string, sustainId: string): Promise<SyncDto> =>
    unwrap(await commands.syncWithPeer(address, sustainId)),

  // ── the arena ───────────────────────────────────────────────────
  /** Every package, judged **now** against the given install target. */
  library: async (into: string | null): Promise<LibraryDto> =>
    await commands.getLibrary(into),
  /** Stamp, gate, then store. A package that fails its typecheck is refused. */
  publish: async (request: Publication): Promise<InstallDto> =>
    unwrap(await commands.publishPackage(request)),
  /** Install through the same gate a locally-authored artifact faces. */
  install: async (packageId: string, into: string | null): Promise<InstallDto> =>
    unwrap(await commands.installPackage(packageId, into)),
  /** Pay a royalty in juul. Internal credit; never money. */
  payRoyalty: async (packageId: string, amount: number): Promise<RoyaltyDto> =>
    unwrap(await commands.payRoyalty(packageId, amount)),

  // ── quorum ──────────────────────────────────────────────────────────
  /** Every shared Sustain's body, and whether it could be written to now. */
  bodies: async (): Promise<BodyDto[]> => await commands.getBodies(),
  /** Declare a Sustain co-owned by these node keys. */
  shareOwnership: async (sustainId: string, withKeys: string[]): Promise<string[]> =>
    unwrap(await commands.shareOwnership(sustainId, withKeys)),

  // ── packages over the wire ────────────────────────────────────────
  /** What each connected peer is offering. Only peers you chose to add. */
  shelves: async (): Promise<PeerShelfDto[]> => await commands.getPeerShelves(),
  /** Fetch one package by content hash. Records it; does not install it. */
  fetchPackage: async (address: string, contentHash: string): Promise<string> =>
    unwrap(await commands.fetchPackage(address, contentHash)),

  // ── orders ────────────────────────────────────────────────────
  /** What this node has acquired. */
  orders: async (): Promise<OrderDto[]> => await commands.getOrders(),
  /** Acquire a package — settles its royalty in juul. Never money. */
  order: async (packageId: string, on: number): Promise<OrderDto> =>
    unwrap(await commands.placeOrder(packageId, on)),
};

/* ── reading real state, honestly ─────────────────────────────────────────
 * The engine's state is an untyped JSON document by design — a Sustain's shape
 * is DECLARED, not compiled in, so the UI cannot have a static type for it.
 * These readers therefore return `null` for absent rather than 0, so a missing
 * dimension renders as "—" and never as a confident zero.
 */

export type Pocket = {
  name: string;
  allocated: number;
  spent: number;
  left: number;
};

const num = (v: unknown): number | null => (typeof v === "number" ? v : null);

export function liquidBalance(state: unknown): number | null {
  const f = (state as Record<string, any>)?.finances;
  return num(f?.liquid?.balance);
}

export function pockets(state: unknown): Pocket[] {
  const raw = (state as Record<string, any>)?.finances?.pockets;
  if (!raw || typeof raw !== "object") return [];
  return Object.entries(raw as Record<string, any>).map(([name, p]) => {
    const allocated = num(p?.allocated) ?? 0;
    const spent = num(p?.spent) ?? 0;
    return { name, allocated, spent, left: allocated - spent };
  });
}

export const fmt = (n: number | null | undefined): string => {
  if (n === null || n === undefined) return "—";
  // Negative zero is a real f64 value and renders as "-0.00", which reads as a
  // number that moved. It did not.
  const v = Object.is(n, -0) ? 0 : n;
  return v.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
};

/** Children of a Sustain, from the composition links the registry holds. */
export const childrenOf = (world: WorldDto | undefined, id: string): SustainSummary[] =>
  (world?.sustains ?? []).filter((s) => s.parent === id);
