/**
 * THE LOCK SCREEN — unlock the local identity, or enrol one.
 *
 * ★★★ This is a **door, not a curtain.** The cockpit behind it is not merely
 * hidden while locked: the host has no private key, so it has no principal, so
 * `admit()` refuses every call with `not_authenticated`. Deleting this screen
 * would not unlock anything.
 *
 * ★★ The passphrase is not compared against a stored answer. It derives a key
 * through PBKDF2-HMAC-SHA256, and either that key decrypts the private half or
 * it does not — so a wrong passphrase fails closed with nothing to invert, and
 * the recovered key is then made to **prove itself** against the public half on
 * file.
 *
 * ★ One error message for a wrong passphrase and for a tampered file, because
 * telling them apart would tell an attacker which half they got right.
 */
import { createSignal, Show } from "solid-js";
import {
  Button,
  Caption,
  Card,
  Cluster,
  ErrorState,
  Field,
  Label,
  Meta,
  Note,
  Readout,
  sx as S,
  vars,
} from "../ui";
import { engine, type IdentityDto } from "../lib/engine";

export default function Lock(props: {
  identity: IdentityDto;
  onUnlocked: (id: IdentityDto) => void;
}) {
  const [handle, setHandle] = createSignal("bg.myc");
  const [pass, setPass] = createSignal("");
  const [confirm, setConfirm] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [failure, setFailure] = createSignal<string | null>(null);

  const enrolling = () => !props.identity.enrolled;

  const go = async () => {
    setFailure(null);
    if (enrolling() && pass() !== confirm()) {
      setFailure("the two passphrases do not match");
      return;
    }
    setBusy(true);
    try {
      const id = enrolling()
        ? await engine.enrol(handle(), pass())
        : await engine.unlock(pass());
      setPass("");
      setConfirm("");
      props.onUnlocked(id);
    } catch (e) {
      // ★ The host's own words. A friendlier lie here would be a worse one.
      setFailure(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class={S.lockFrame}>
      <div class={S.lockPanel}>
        <div class={S.brand}>MYCELIUM</div>
        <Note>
          <Caption>
            {enrolling()
              ? "no identity on this machine yet · mint one"
              : "the local identity is locked"}
          </Caption>
        </Note>

        <Card title={enrolling() ? "enrol · mint a keypair" : "unlock"}>
          <Show when={!enrolling()}>
            <Readout label="identity">{props.identity.handle ?? "—"}</Readout>
          </Show>
          <Show when={enrolling()}>
            <Field label="handle">
              <input
                class={S.input}
                value={handle()}
                onInput={(e) => setHandle(e.currentTarget.value)}
              />
            </Field>
          </Show>

          <Note>
            <Field label="passphrase">
              <input
                class={S.input}
                type="password"
                autofocus
                value={pass()}
                onInput={(e) => setPass(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !enrolling()) void go();
                }}
              />
            </Field>
          </Note>
          <Show when={enrolling()}>
            <Note>
              <Field label="passphrase, again">
                <input
                  class={S.input}
                  type="password"
                  value={confirm()}
                  onInput={(e) => setConfirm(e.currentTarget.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void go();
                  }}
                />
              </Field>
            </Note>
          </Show>

          <Note gap="lg">
            <Cluster>
              <Button onClick={go} disabled={busy() || pass().length === 0}>
                {busy()
                  ? enrolling()
                    ? "minting…"
                    : "deriving the key…"
                  : enrolling()
                    ? "enrol"
                    : "unlock"}
              </Button>
            </Cluster>
          </Note>

          <Show when={failure()}>
            {(f) => (
              <Note>
                <ErrorState>{f()}</ErrorState>
              </Note>
            )}
          </Show>

          <Note gap="lg">
            <Label>what this is</Label>
            <Caption>
              The identity is an <strong>ed25519 keypair</strong>, and the handle is its name.
              The passphrase never leaves this machine and is never stored — it derives a key
              through{" "}
              <code>{props.identity.kdf ?? "pbkdf2-hmac-sha256"}</code>
              {props.identity.iterations
                ? ` at ${props.identity.iterations.toLocaleString()} iterations`
                : ""}
              , and that key either decrypts the private half or it does not. Nothing in the
              cockpit can act until it does: the gate refuses every call with{" "}
              <code>not_authenticated</code> while there is no principal.
            </Caption>
          </Note>

          <Show when={enrolling()}>
            <Note>
              <Caption style={{ color: vars.color.warn }}>
                <strong>There is no recovery.</strong> No reset, no second device, no phrase to
                write down but this one. Losing the passphrase loses the identity — said now
                rather than discovered later.
              </Caption>
            </Note>
          </Show>
        </Card>

        <Note>
          <Meta>sustena-core · embedded · no server, no network</Meta>
        </Note>
      </div>
    </div>
  );
}
