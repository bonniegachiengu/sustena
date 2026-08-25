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
  type SmsSweep,
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
  type InferenceDto,
  type AggregateDto,
  type SustainDto,
  type SustainSummary,
  type TemplateId,
  type WorldDto,
} from "../bindings";

export type {
  AccessDto,
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
  SmsSweep,
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
  smsPermission: async (): Promise<string> => unwrap(await commands.smsPermissionState()),
  smsRequestPermission: async (): Promise<string> => unwrap(await commands.smsRequestPermission()),
  /** Reads texts already on the phone. 0 days means all of them. */
  smsImport: async (sustainId: string, sinceDays: number): Promise<SmsSweep> =>
    unwrap(await commands.smsImportInbox(sustainId, sinceDays)),
  /** Hands over what arrived while the app was shut, and clears it. */
  smsDrain: async (sustainId: string): Promise<SmsSweep> =>
    unwrap(await commands.smsDrainQueue(sustainId)),

  declareSource: async (id: string, label: string, minutes: number | null): Promise<null> =>
    unwrap(await commands.declareSource(id, label, minutes)),
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
  ): Promise<GateResult> =>
    unwrap(await commands.orchieConfirm(sustainId, operator, params, messageId, description)),

  /** "remember this format" — synthesise a rule from a confirmed correction. */
  learnRule: async (messageId: string, operator: string, params: JsonValue): Promise<string> =>
    unwrap(await commands.learnRule(messageId, operator, params)),
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
