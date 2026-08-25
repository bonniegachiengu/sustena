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
import { keyboardAware, watchViewport } from "../lib/viewport";
import * as O from "../ui/orchie.css";
import { onMount } from "solid-js";

export default function Lock(props: {
  identity: IdentityDto;
  onUnlocked: (id: IdentityDto) => void;
  /**
   * ★★★ Which face is asking. This screen is the FIRST thing anyone sees, and
   * for a long time it showed everybody the same thing: the cockpit's own
   * panel, at cockpit density, headed MYCELIUM, explaining ed25519 keypairs and
   * PBKDF2 at 600,000 iterations. On a laptop that is exactly right -- the
   * person opening the cockpit wants to know what the door is made of.
   *
   * On a phone it is the wrong door entirely. Orchie's users are Bonnie's mum,
   * Cira and Epha; the first screen of the app was a locked technical briefing
   * they have no way to act on, before they could reach the one thing they are
   * here to do. Same door, same key, same refusal on a wrong passphrase --
   * different clothes.
   */
  face?: "mycelium" | "orchie";
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

  onMount(watchViewport);

  // ═══ the phone door ═══════════════════════════════════════════════════════
  if (props.face === "orchie") {
    return (
      <div class={O.frame} data-face="orchie">
        <div class={O.column}>
          <div class={O.header}>
            <span class={O.brand}>ORCHIE</span>
          </div>

          <div class={O.card}>
            <h2 class={O.cardTitle}>
              {enrolling() ? "set a passphrase" : "welcome back"}
            </h2>
            <p class={O.body}>
              {enrolling()
                ? "This phone does not have your key yet. Choose a passphrase. It never leaves this phone, and it is the only way in."
                : "Enter your passphrase to open your household."}
            </p>

            <Show when={enrolling()}>
              <input
                ref={keyboardAware}
                class={O.input}
                type="text"
                autocapitalize="none"
                autocomplete="off"
                placeholder="a name for this phone"
                value={handle()}
                onInput={(e) => setHandle(e.currentTarget.value)}
              />
            </Show>

            <input
              ref={keyboardAware}
              class={O.input}
              type="password"
              enterkeyhint={enrolling() ? "next" : "go"}
              autocapitalize="none"
              autocomplete={enrolling() ? "new-password" : "current-password"}
              placeholder="passphrase"
              value={pass()}
              onInput={(e) => setPass(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !enrolling()) {
                  e.currentTarget.blur();
                  void go();
                }
              }}
            />

            <Show when={enrolling()}>
              <input
                ref={keyboardAware}
                class={O.input}
                type="password"
                enterkeyhint="go"
                autocapitalize="none"
                autocomplete="new-password"
                placeholder="the same passphrase again"
                value={confirm()}
                onInput={(e) => setConfirm(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.currentTarget.blur();
                    void go();
                  }
                }}
              />
            </Show>

            <button
              class={`${O.action.primary} ${O.actionWide}`}
              onClick={() => void go()}
              disabled={busy() || pass() === ""}
            >
              <Show when={busy()} fallback={enrolling() ? "create it" : "open"}>
                <span class={O.working} /> checking…
              </Show>
            </button>

            {/* ★ The host's own words, unchanged. A friendlier lie here would
                be a worse one -- and on a phone it would be the only thing
                standing between a person and thinking the app is broken. */}
            <Show when={failure()}>
              {(f) => <div class={O.errorBox}>{f()}</div>}
            </Show>
          </div>

          {/* ★★ The technical account is not deleted, only folded away. It is
              true and it matters; it is simply not the first thing a person
              needs in order to get in. */}
          <Details />
        </div>
      </div>
    );
  }

  // ═══ the cockpit door, unchanged ══════════════════════════════════════════
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
              Your identity is a keypair and the handle is its name. The passphrase never
              leaves this machine and is never stored. It unlocks your private key, and
              nothing here can act until it does.
            </Caption>
          </Note>

          <Show when={enrolling()}>
            <Note>
              <Caption style={{ color: vars.color.warn }}>
                <strong>There is no recovery.</strong> No reset and no second device. If you
                lose this passphrase you lose the identity.
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

/**
 * ★★ The same account of what the door is made of, folded away.
 *
 * Not softened and not removed -- a person who wants to know how their key is
 * held should be able to find out, and on the cockpit face this text is on
 * screen by default. It is behind a tap here only because it is not the thing
 * standing between someone and their household.
 */
function Details() {
  const [open, setOpen] = createSignal(false);
  return (
    <>
      <button class={O.linkish} onClick={() => setOpen((o) => !o)}>
        {open() ? "▾" : "▸"} how is my passphrase kept?
      </button>
      <Show when={open()}>
        <div class={O.card}>
          <p class={O.caption}>
            Your identity is a keypair and the handle is its name. The passphrase never leaves
            this phone and is never stored anywhere. It unlocks your private key.
          </p>
          <p class={O.caption}>
            Nothing in the app can act until it does. While it is locked, every request is
            refused.
          </p>
          <p class={O.caption}>
            A wrong passphrase and a damaged file give the same message on purpose. Telling them
            apart would tell someone which half they had guessed right.
          </p>
        </div>
      </Show>
    </>
  );
}
