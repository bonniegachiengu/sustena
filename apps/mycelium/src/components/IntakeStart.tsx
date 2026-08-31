/**
 * WHERE THE RECORD BEGINS — the intake boundary, as a control.
 *
 * ★★★ **Why this needs a surface at all.** A phone carries years of texts.
 * Starting Sustena on a Tuesday does not mean the household began when the
 * handset did, and importing two and a half thousand old messages opens a
 * classify queue nobody asked for. The boundary is the fix; showing it is what
 * makes the boundary honest. A cutoff a person cannot see is a household
 * silently missing things with no way to discover why.
 *
 * ★★ **One component, both faces.** The phone and the cockpit ask the same
 * question and must not drift into two answers. It takes its own styling by
 * face rather than being forked, for the same reason there is one classifier
 * behind several doors.
 *
 * ★ Local time in, unix seconds out. A person picks a moment in the timezone
 * they are standing in; the engine stores the instant, which is what survives a
 * timezone the phone has since left.
 */

import { createResource, createSignal, Show } from "solid-js";

import { engine } from "../lib/engine";

/** `<input type="datetime-local">` wants `YYYY-MM-DDTHH:mm` in LOCAL time. */
function toLocalInput(unixSeconds: number): string {
  const d = new Date(unixSeconds * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
    `T${pad(d.getHours())}:${pad(d.getMinutes())}`
  );
}

function fromLocalInput(value: string): number | null {
  if (!value) return null;
  const ms = new Date(value).getTime();
  return Number.isFinite(ms) ? Math.floor(ms / 1000) : null;
}

/** Readable, and honest about the timezone it is showing. */
export function describeStart(unixSeconds: number | null): string {
  if (unixSeconds === null) {
    return "everything on this phone";
  }
  return new Date(unixSeconds * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

export function IntakeStart(props: {
  face: "orchie" | "cockpit";
  /** Called after a successful change, so a caller can refresh its counts. */
  onChanged?: () => void;
}) {
  const [current, { refetch }] = createResource(() => engine.intakeStart());
  const [draft, setDraft] = createSignal<string>("");
  const [busy, setBusy] = createSignal(false);
  const [failure, setFailure] = createSignal<string | null>(null);
  const [editing, setEditing] = createSignal(false);

  const open = () => {
    const now = current();
    setDraft(toLocalInput(now ?? Math.floor(Date.now() / 1000)));
    setFailure(null);
    setEditing(true);
  };

  const save = async (value: number | null) => {
    setBusy(true);
    setFailure(null);
    try {
      await engine.setIntakeStart(value);
      await refetch();
      setEditing(false);
      props.onChanged?.();
    } catch (e) {
      // ★ The engine's own words. A friendlier lie about a money boundary
      //   would be a worse one.
      setFailure(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  const phone = () => props.face === "orchie";

  return (
    <div data-intake-start={props.face}>
      <p>
        <strong>reading from</strong>{" "}
        <span>{describeStart(current() ?? null)}</span>
      </p>

      <Show
        when={editing()}
        fallback={
          <p>
            <button type="button" onClick={open} disabled={busy()}>
              {current() === null || current() === undefined
                ? "start from a date instead"
                : "change the date"}
            </button>
            <Show when={current() !== null && current() !== undefined}>
              {" · "}
              <button type="button" onClick={() => void save(null)} disabled={busy()}>
                read everything
              </button>
            </Show>
          </p>
        }
      >
        <p>
          <input
            type="datetime-local"
            value={draft()}
            onInput={(e) => setDraft(e.currentTarget.value)}
          />{" "}
          <button
            type="button"
            onClick={() => void save(fromLocalInput(draft()))}
            disabled={busy() || draft() === ""}
          >
            {busy() ? "saving…" : "read from here"}
          </button>{" "}
          <button type="button" onClick={() => setEditing(false)} disabled={busy()}>
            cancel
          </button>
        </p>
      </Show>

      {/* ★★★ Both halves of the truth, because either alone misleads. It stops
          the backlog arriving AND it never removes what is already recorded --
          a person must not fear that moving this erases their household. */}
      <p>
        {phone()
          ? "texts older than this are not read off your phone at all. Nothing already recorded is removed."
          : "messages older than this never cross the intake boundary — not read, not stored, not queued. Nothing already recorded is removed."}
      </p>

      <Show when={failure()}>{(f) => <p role="alert">{f()}</p>}</Show>
    </div>
  );
}
