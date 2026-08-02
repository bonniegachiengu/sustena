/**
 * apps/web/src/pages/OrchieShell.jsx
 *
 * ORCHIE — the phone-first, event-first curated surface. This is a NEW,
 * separate surface from every /devui panel (those are proto-MYCELIUM, the
 * orchestrator/dev cockpit at "/") — Orchie is the person-facing view: the
 * system comes to YOU, one thing at a time, ranked by what actually needs
 * you right now. It calls GET /orchie/compose — the Curated UI engine's
 * compose(r) — and renders exactly what it returns. No sustain-specific
 * strings live here; whatever compose() selects is what renders.
 *
 * Effect-first capture (§7): tapping a classify_card, or typing a plain
 * narration at the top of the page, opens a CaptureFlow — one question at
 * a time (§6 progressive disclosure), never a form, never a schema. The
 * engine infers the operator + params; the human only ever confirms or
 * answers a single tap-question. Nothing mutates until CONFIRM is tapped.
 */
import React, { useEffect, useState, useCallback, useRef } from 'react';
import { useSearchParams, Link } from 'react-router-dom';
import { Capacitor } from '@capacitor/core';
import { api } from '../lib/api';
import LoginGate from '../components/LoginGate';
import {
  checkSmsPermission, requestSmsPermission, backfillInbox, startLivePolling, stopLivePolling,
  ensureNotificationPermission, setNativeAuthContext, consumePendingClassifyTarget, RESYNC_WINDOWS,
} from '../lib/smsCapture';

const LAST_SUSTAIN_KEY = 'sustena_orchie_last_sustain';

/**
 * Which sustain Orchie composes for, plus the full list for a picker.
 *
 * The bug this replaces: the old version defaulted to `list[0]` from
 * GET /devui/sustains, which orders `created_at DESC` (newest first) --
 * for an owner whose Homestead predates their habitats (the normal case,
 * since habitats get provisioned onto an existing Homestead), that put a
 * just-created, curated-widget-less habitat first and the actual
 * Homestead (with real widgets) further down. compose() then honestly
 * returned "no curated widgets declared for this sustain yet" -- correct
 * for THAT sustain, wrong as a silent default no one could see or change.
 *
 * Fix, in priority order:
 *   1. `?sustain=` in the URL always wins (explicit, shareable, unchanged).
 *   2. A previously-chosen sustain (localStorage) if it's still in the
 *      user's list -- fast path for a returning visit, no re-probing.
 *   3. First-ever visit: probe every owned sustain via the existing,
 *      read-only GET /orchie/compose (budget=1, cheap) in parallel, and
 *      default to whichever has the most curated widgets eligible
 *      (`candidates_considered`) -- generic, no "homestead" string
 *      anywhere, works for any future sustain type the same way.
 * The choice is always visible and always changeable via the picker
 * (`SustainPicker` below), which is the other half of this fix -- a
 * silently-wrong default with no way to override it is the actual bug,
 * not just which sustain happened to be picked.
 */
function useSustainPicker(token) {
  const [searchParams, setSearchParams] = useSearchParams();
  const explicit = searchParams.get('sustain');
  const [sustains, setSustains] = useState([]);
  const [sustainId, setSustainIdState] = useState(explicit);
  const [resolving, setResolving] = useState(true);

  useEffect(() => {
    // Without this guard, a not-yet-signed-in mount fires GET
    // /devui/sustains with no Authorization header -> a real 401 -> api.js's
    // handleUnauthorized() clears the (already-absent) token and reloads
    // the page -> the same thing happens again on the fresh load ->
    // infinite reload loop, before LoginGate ever gets a chance to render.
    // Re-runs (token in the dep array) the moment LoginGate's onSignedIn
    // sets a real token, so sustains actually load right after signing in
    // rather than needing a manual refresh.
    if (!token) {
      setSustains([]);
      setSustainIdState(explicit || null);
      setResolving(false);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const resp = await api.get('/devui/sustains');
        const list = resp?.data?.sustains || [];
        if (cancelled) return;
        setSustains(list);

        if (explicit) {
          setSustainIdState(explicit);
          setResolving(false);
          return;
        }

        const stored = typeof window !== 'undefined' ? window.localStorage.getItem(LAST_SUSTAIN_KEY) : null;
        if (stored && list.some(s => s.id === stored)) {
          setSustainIdState(stored);
          setResolving(false);
          return;
        }

        if (list.length === 0) {
          setSustainIdState(null);
          setResolving(false);
          return;
        }
        if (list.length === 1) {
          setSustainIdState(list[0].id);
          setResolving(false);
          return;
        }

        // Multiple sustains, no explicit/stored choice: probe each one's
        // real widget eligibility rather than guessing from list order.
        const probes = await Promise.allSettled(
          list.map(s => api.get(`/orchie/compose?sustain_id=${encodeURIComponent(s.id)}&device=phone&budget=1`))
        );
        let best = list[0];
        let bestScore = -1;
        probes.forEach((res, i) => {
          const considered = res.status === 'fulfilled' ? (res.value?.candidates_considered || 0) : -1;
          if (considered > bestScore) { bestScore = considered; best = list[i]; }
        });
        if (!cancelled) {
          setSustainIdState(best.id);
          setResolving(false);
        }
      } catch {
        if (!cancelled) { setSustainIdState(null); setResolving(false); }
      }
    })();
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token]);

  const chooseSustain = useCallback((id) => {
    setSustainIdState(id);
    if (typeof window !== 'undefined') window.localStorage.setItem(LAST_SUSTAIN_KEY, id);
    const next = new URLSearchParams(searchParams);
    next.set('sustain', id);
    setSearchParams(next, { replace: true });
  }, [searchParams, setSearchParams]);

  return { sustainId, sustains, resolving, chooseSustain };
}

function SustainPicker({ sustainId, sustains, onChoose }) {
  if (!sustains || sustains.length < 2) return null;
  // Multiple sustains can share the same label (e.g. six "Habitat"
  // instances) -- a short id suffix keeps every option distinguishable
  // without needing a per-instance display name the backend doesn't have.
  const dupeLabels = new Set(
    sustains.map(s => s.label).filter((l, i, arr) => arr.indexOf(l) !== arr.lastIndexOf(l))
  );
  return (
    <select
      value={sustainId || ''}
      onChange={e => onChoose(e.target.value)}
      style={{
        fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.04em',
        color: 'var(--text-muted)', background: 'var(--bg-surface)',
        border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
        padding: '4px 8px', maxWidth: 150,
      }}
    >
      {sustains.map(s => (
        <option key={s.id} value={s.id}>
          {s.label}{dupeLabels.has(s.label) ? ` · ${s.id.slice(0, 6)}` : ''}
        </option>
      ))}
    </select>
  );
}

function WhyReveal({ widget }) {
  const [open, setOpen] = useState(false);
  return (
    <div style={{ marginTop: 10 }}>
      <button
        onClick={() => setOpen(o => !o)}
        style={{
          background: 'none', border: 'none', padding: 0, cursor: 'pointer',
          fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.05em',
          color: 'var(--text-muted)', textDecoration: 'underline dotted',
        }}
      >
        why am I seeing this?
      </button>
      {open && (
        <div style={{
          marginTop: 8, padding: '10px 12px', borderRadius: 'var(--radius-sm)',
          background: 'var(--bg-raised)', border: '1px solid var(--border)',
        }}>
          <div style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.5 }}>
            {widget.why}
          </div>
          <div style={{
            marginTop: 8, display: 'flex', gap: 14,
            fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', letterSpacing: '0.05em',
          }}>
            <span>urgency {widget.urgency}</span>
            <span>relevance {widget.relevance}</span>
            <span>score {widget.score}</span>
            <span>cost {widget.cost}</span>
          </div>
        </div>
      )}
    </div>
  );
}

/* ─── Effect-first capture (§7) — one question at a time, never a form ─── */

const bigTapButton = {
  fontFamily: 'var(--ui)', fontSize: 13, fontWeight: 500,
  color: 'var(--text-primary)', background: 'var(--bg-overlay)',
  border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
  padding: '12px 16px', cursor: 'pointer', minWidth: 88,
};
const confirmButton = {
  fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 700, letterSpacing: '0.06em',
  color: 'var(--bg-base)', background: 'var(--teal)',
  border: 'none', borderRadius: 'var(--radius-sm)', padding: '12px 20px', cursor: 'pointer',
};
const cancelButton = {
  fontFamily: 'var(--mono)', fontSize: 11, letterSpacing: '0.06em',
  color: 'var(--text-muted)', background: 'none',
  border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)', padding: '12px 18px', cursor: 'pointer',
};
const mutedText = { fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-muted)' };
const questionText = { fontFamily: 'var(--ui)', fontSize: 14, color: 'var(--text-primary)' };

/** A plain-language summary of what a confirm is ABOUT to do, captured
 *  before the /confirm call overwrites `payload` with the response body --
 *  used for the success message so it can still say "Ksh X to food"
 *  rather than a generic "done". */
function describeOutcome(readyPayload) {
  const amount = readyPayload?.params?.amount;
  if (readyPayload?.operator === 'budget.record_income') {
    return amount != null ? `Ksh ${amount} received` : 'income recorded';
  }
  const pocket = readyPayload?.params?.pocket_name;
  if (amount != null && pocket) return `Ksh ${amount} to ${pocket}`;
  if (amount != null) return `Ksh ${amount}`;
  return null;
}

/**
 * A plain +/- money-direction cue, styled entirely in Sustena's OWN palette
 * (globals.css's --teal/--amber) -- explicitly NOT VOS's safety-orange
 * (outgoing) / magenta (incoming) convention, which does not belong in
 * Sustena. Reuses --teal for "+" (received) the same way this file already
 * uses it for CONFIRM/committed-success, and --amber for "-" (sent) the
 * same way it's already Sustena's default/primary accent everywhere else
 * (message bubbles, active nav, the cursor) -- both are colors this app
 * already assigns real meaning to, not new ones invented for this cue.
 */
function DirectionBadge({ direction }) {
  if (direction === 'received') {
    return <span style={{ fontFamily: 'var(--mono)', fontWeight: 700, color: 'var(--teal)' }}>+</span>;
  }
  if (direction === 'sent') {
    return <span style={{ fontFamily: 'var(--mono)', fontWeight: 700, color: 'var(--amber)' }}>−</span>;
  }
  return null;
}

/**
 * field="amount" is the one disambiguation kind with no enumerable
 * options -- an amount is typed, not tapped. Never a dead end: this
 * renders whenever infer() couldn't recover a figure from the message
 * itself (or a narration), which is now a genuine last resort (see
 * effect_capture.infer()'s own docstring) rather than the "no amount
 * could be recovered" wall Bonnie hit live.
 */
function AmountEntry({ question, why, onSubmit }) {
  const [value, setValue] = useState('');
  const submit = () => { if (value.trim()) onSubmit(value.trim()); };
  return (
    <div>
      <div style={questionText}>{question}</div>
      <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
        <input
          type="number"
          inputMode="decimal"
          value={value}
          onChange={e => setValue(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter') submit(); }}
          placeholder="e.g. 500"
          autoFocus
          style={{
            flex: 1, fontFamily: 'var(--mono)', fontSize: 15, color: 'var(--text-primary)',
            background: 'var(--bg-surface)', border: '1px solid var(--border-mid)',
            borderRadius: 'var(--radius-sm)', padding: '10px 12px',
          }}
        />
        <button onClick={submit} style={confirmButton}>OK</button>
      </div>
      <div style={{ ...mutedText, marginTop: 10 }}>{why}</div>
    </div>
  );
}

/**
 * The pocket-name disambiguation, extended with a "+ NEW" tile per
 * Bonnie's ask -- creating a pocket goes through a real, gated operator
 * (budget.add_pocket, new), never a side-write. On success, the REAL
 * final pocket name (server-normalised -- see budget.add_pocket's own
 * docstring for why a typed "holiday fund" comes back as "holiday_fund")
 * is what the flow continues with, and a brief inline confirmation is
 * shown before handing off (see confirm/success feedback discipline
 * applied throughout this file, 2 Aug 2026 pass).
 */
function PocketPicker({ question, why, options, sustainId, onPick }) {
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState('');
  const [status, setStatus] = useState('idle'); // idle | saving | created | error
  const [error, setError] = useState('');
  const [createdName, setCreatedName] = useState('');

  const submitNew = async () => {
    if (!name.trim() || status === 'saving') return;
    setStatus('saving');
    try {
      const data = await api.post('/orchie/capture/confirm', {
        sustain_id: sustainId, operator: 'budget.add_pocket', params: { pocket_name: name.trim() },
      });
      if (data.result.status === 'ok') {
        const realName = data.result.data.pocket;
        setCreatedName(realName);
        setStatus('created');
        setTimeout(() => onPick(realName), 900);
      } else {
        setStatus('error');
        setError(data.result.reason || 'could not create the pocket');
      }
    } catch (e) {
      setStatus('error');
      setError(e.message || 'could not reach orchie');
    }
  };

  return (
    <div>
      <div style={questionText}>{question}</div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 12 }}>
        {options.map(o => (
          <button key={o.value} onClick={() => onPick(o.value)} style={bigTapButton}>{o.label}</button>
        ))}
        {!creating && status === 'idle' && (
          <button
            onClick={() => setCreating(true)}
            style={{ ...bigTapButton, borderStyle: 'dashed', borderColor: 'var(--teal-border)', color: 'var(--teal)' }}
          >
            + NEW
          </button>
        )}
      </div>

      {creating && status !== 'created' && (
        <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
          <input
            type="text" value={name}
            onChange={e => setName(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') submitNew(); }}
            placeholder="pocket name, e.g. Holiday Fund"
            autoFocus
            disabled={status === 'saving'}
            style={editableInput}
          />
          <button onClick={submitNew} style={confirmButton} disabled={status === 'saving'}>
            {status === 'saving' ? 'creating…' : 'CREATE'}
          </button>
        </div>
      )}

      {status === 'created' && (
        <div className="pulse" style={{
          marginTop: 12, display: 'flex', alignItems: 'center', gap: 8,
          fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 700, color: 'var(--teal)',
        }}>
          <span style={{ width: 8, height: 8, borderRadius: '50%', background: 'var(--teal)' }} />
          ✓ '{createdName}' created — continuing…
        </div>
      )}

      {status === 'error' && (
        <div style={{
          marginTop: 8, display: 'flex', alignItems: 'center', gap: 8,
          fontFamily: 'var(--mono)', fontSize: 12, fontWeight: 700, color: 'var(--danger)',
        }}>
          <span style={{ width: 7, height: 7, borderRadius: '50%', background: 'var(--danger)' }} />
          ✗ {error}
        </div>
      )}

      <div style={{ ...mutedText, marginTop: 10 }}>{why}</div>
    </div>
  );
}

/**
 * The human escape hatch a classify card needs when it's asking a question
 * (which pocket / what amount / anything else) about a captured message
 * that turns out not to be a transaction at all (Bonnie, 2 Aug 2026: a
 * real M-Pesa failure notice -- "Failed. The till number entered is
 * incorrect..." -- reached this exact screen because the informational
 * filter didn't yet recognise its wording). Rendered under EVERY
 * needs_disambiguation branch in CaptureFlow (amount / pocket_name /
 * generic), never inside 'ready' -- once amount+pocket are both resolved
 * the person has already effectively confirmed it IS a transaction.
 *
 * Only shown when a real messageId exists (a free-text narration has no
 * underlying message to mark). Calls the real, gated
 * POST /orchie/capture/messages/{id}/not-a-transaction -- never a raw
 * write, never calls execute_operator (there's nothing to book).
 */
function NotATransactionControl({ sustainId, messageId, onMarked }) {
  const [status, setStatus] = useState('idle'); // idle | marking | done | error
  const [error, setError] = useState(null);

  // Same discipline as CaptureFlow's own committed-phase auto-dismiss: the
  // "done -> tell the parent" transition lives in an effect with its own
  // cleanup, not a bare setTimeout in the click handler, so a fast unmount
  // (navigate away / switch sustain / list refresh) inside the 1.2s window
  // can never fire a stale onMarked() against an already-gone widget.
  useEffect(() => {
    if (status !== 'done') return undefined;
    const t = setTimeout(() => onMarked?.(), 1200);
    return () => clearTimeout(t);
  }, [status, onMarked]);

  const mark = async () => {
    setStatus('marking');
    setError(null);
    try {
      const data = await api.post(`/orchie/capture/messages/${encodeURIComponent(messageId)}/not-a-transaction`, {
        sustain_id: sustainId,
      });
      if (data.marked) {
        setStatus('done');
      } else {
        setStatus('error');
        setError('could not mark it — it may already be resolved');
      }
    } catch (e) {
      setStatus('error');
      setError(e.message || 'could not reach orchie');
    }
  };

  if (status === 'marking') {
    return (
      <div className="pulse" style={{
        marginTop: 10, display: 'flex', alignItems: 'center', gap: 8,
        fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-muted)',
      }}>
        <span style={{ width: 7, height: 7, borderRadius: '50%', background: 'var(--amber)' }} />
        marking…
      </div>
    );
  }

  if (status === 'done') {
    return (
      <div style={{
        marginTop: 10, display: 'flex', alignItems: 'center', gap: 8,
        fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 700, color: 'var(--teal)',
      }}>
        <span style={{ width: 7, height: 7, borderRadius: '50%', background: 'var(--teal)' }} />
        ✓ not a transaction — cleared
      </div>
    );
  }

  return (
    <div style={{ marginTop: 10 }}>
      <button
        onClick={mark}
        style={{
          background: 'none', border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
          padding: '8px 14px', cursor: 'pointer',
          fontFamily: 'var(--mono)', fontSize: 11, letterSpacing: '0.03em', color: 'var(--text-muted)',
        }}
      >
        not a transaction — ignore
      </button>
      {status === 'error' && (
        <div style={{ marginTop: 6, fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--danger)' }}>{error}</div>
      )}
    </div>
  );
}

const fieldLabel = {
  // Reclassified from --text-dim to --text-muted (2 Aug 2026 legibility
  // pass): a field label ("AMOUNT", "DESCRIPTION / MERCHANT") is content
  // the person must read to correct a capture, not decoration -- it
  // belongs at the ~4.5:1 body/label tier, not the ~3:1 decorative one.
  // Weight nudged up one step (400->500) since the light mono weight was
  // part of what made this hard to read at 9px even before the color fix.
  fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500, letterSpacing: '0.06em',
  color: 'var(--text-muted)', textTransform: 'uppercase', marginBottom: 4, display: 'block',
};
const editableInput = {
  width: '100%', fontFamily: 'var(--ui)', fontSize: 14, color: 'var(--text-primary)',
  background: 'var(--bg-surface)', border: '1px solid var(--border-mid)',
  borderRadius: 'var(--radius-sm)', padding: '9px 11px',
};
const directionToggle = (active) => ({
  fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 700, letterSpacing: '0.05em',
  padding: '9px 16px', borderRadius: 'var(--radius-sm)', cursor: 'pointer',
  border: `1px solid ${active ? 'var(--teal-border)' : 'var(--border-mid)'}`,
  background: active ? 'var(--teal-glow)' : 'none',
  color: active ? 'var(--teal)' : 'var(--text-muted)',
});

/**
 * The "real degrees of freedom" review, per Bonnie's explicit ask: a
 * captured transaction (correctly parsed or not) is reviewed HERE, in the
 * moment, with amount / description / direction / pocket all genuinely
 * editable before the single CONFIRM tap -- not just "pick a pocket."
 * Every edit re-runs infer() with the correction folded into `known` (see
 * CaptureFlow's setKnownField/clearKnownFields/setDirection), so the
 * result shown is always what would actually be committed, never a local-
 * only preview that could drift from what CONFIRM sends.
 */
function ReadyReview({ payload, onSetAmount, onSetDescription, onSetDirection, onChangePocket, onChangePocketFromHistory, onConfirm, onCancel }) {
  const [amount, setAmount] = useState('');
  const [description, setDescription] = useState('');

  useEffect(() => {
    setAmount(payload.params?.amount != null ? String(payload.params.amount) : '');
    setDescription(payload.description || '');
  }, [payload]);

  const isIncome = payload.operator === 'budget.record_income';
  const pocketRelevant = !isIncome;

  const commitAmount = () => { if (amount !== '' && String(payload.params?.amount) !== amount) onSetAmount(amount); };
  const commitDescription = () => { if (description !== (payload.description || '')) onSetDescription(description); };

  return (
    <div>
      <div style={questionText}>{payload.why}</div>
      {payload.from_history && (
        <div style={{ ...mutedText, marginTop: 4, color: 'var(--teal)' }}>
          pre-filled from how you classified this before — tap CONFIRM, or CHANGE if it's different this time
        </div>
      )}

      <div style={{ display: 'flex', gap: 8, marginTop: 14 }}>
        <button onClick={() => onSetDirection('out')} style={directionToggle(!isIncome)}>MONEY OUT</button>
        <button onClick={() => onSetDirection('in')} style={directionToggle(isIncome)}>MONEY IN</button>
      </div>

      <div style={{ marginTop: 12 }}>
        <span style={fieldLabel}>amount</span>
        <input
          type="number" inputMode="decimal" value={amount}
          onChange={e => setAmount(e.target.value)}
          onBlur={commitAmount}
          onKeyDown={e => { if (e.key === 'Enter') commitAmount(); }}
          style={editableInput}
        />
      </div>

      <div style={{ marginTop: 10 }}>
        <span style={fieldLabel}>description / merchant</span>
        <input
          type="text" value={description}
          onChange={e => setDescription(e.target.value)}
          onBlur={commitDescription}
          onKeyDown={e => { if (e.key === 'Enter') commitDescription(); }}
          placeholder="e.g. NAIVAS SUPERMARKET"
          style={editableInput}
        />
      </div>

      {pocketRelevant && (
        <div style={{ marginTop: 10, display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <div>
            <span style={fieldLabel}>pocket</span>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 13, color: 'var(--text-primary)' }}>
              {payload.params?.pocket_name || '—'}
            </span>
          </div>
          <button
            onClick={payload.from_history ? onChangePocketFromHistory : onChangePocket}
            style={{ ...cancelButton, padding: '6px 12px', fontSize: 10 }}
          >
            CHANGE
          </button>
        </div>
      )}

      <div style={{ display: 'flex', gap: 8, marginTop: 14 }}>
        <button onClick={onConfirm} style={confirmButton}>CONFIRM</button>
        <button onClick={onCancel} style={cancelButton}>CANCEL</button>
      </div>
    </div>
  );
}

function CaptureFlow({ sustainId, widgetId = 'unmapped_capture_classify', messageId, effectText, onClose, onCommitted }) {
  const [phase, setPhase] = useState('loading');
  const [payload, setPayload] = useState(null);
  const [known, setKnown] = useState({});
  const [ignoreHistory, setIgnoreHistory] = useState(false);

  const runInfer = useCallback(async (nextKnown, opts = {}) => {
    setPhase('loading');
    const nextIgnoreHistory = opts.ignoreHistory ?? ignoreHistory;
    try {
      const data = await api.post('/orchie/capture/infer', {
        sustain_id: sustainId, widget_id: widgetId, message_id: messageId, effect_text: effectText,
        known: nextKnown, ignore_history: nextIgnoreHistory,
      });
      setKnown(nextKnown);
      setPayload(data);
      setPhase(data.status);
    } catch (e) {
      setPayload({ why: e.message || 'could not reach orchie' });
      setPhase('error');
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sustainId, widgetId, messageId, effectText, ignoreHistory]);

  useEffect(() => { runInfer({}); }, [runInfer]);

  const answer = (value) => runInfer({ ...known, [payload.field]: value });

  // General correction primitive -- human-in-the-loop degrees of freedom:
  // set one fact (a typed amount/description correction, or an explicit
  // direction/operator override) and re-run infer() with it. Every field
  // infer() can resolve is correctable this way, not just pocket_name.
  const setKnownField = (field, value) => runInfer({ ...known, [field]: value });

  // Clear one or more previously-answered facts and re-run infer() without
  // them, so the normal resolution (text match / history / a fresh ask)
  // runs again -- the general "let me pick something else" primitive.
  const clearKnownFields = (...fields) => {
    const next = { ...known };
    fields.forEach(f => delete next[f]);
    runInfer(next);
  };

  // CHANGE: the pre-filled ("usual") pocket wasn't right this time -- force
  // the normal disambiguation question instead of the history pre-fill.
  const changePocketFromHistory = () => {
    setIgnoreHistory(true);
    runInfer(known, { ignoreHistory: true });
  };

  // A resolved (non-history) pocket wasn't right either -- clear it and
  // let the normal pocket resolution/ask run again.
  const changePocket = () => clearKnownFields('pocket_name');

  // Direction correction -- the core "real degrees of freedom" fix: a
  // capture that resolved (or was about to resolve) as an outgoing spend/
  // allocate can be flipped to income, and back, at any point before
  // CONFIRM. budget.record_income has no pocket_name param at all, so
  // flipping to "money IN" also clears any already-picked pocket.
  const setDirection = (direction) => {
    if (direction === 'in') {
      runInfer({ ...known, operator: 'budget.record_income' });
    } else {
      clearKnownFields('operator', 'pocket_name');
    }
  };

  // The transaction just confirmed, kept alongside `payload` (which gets
  // OVERWRITTEN by the /confirm response below, losing operator/params) so
  // the success message can still say what actually happened.
  const [committedSummary, setCommittedSummary] = useState(null);

  // The ready-phase attempt (operator/params/description), stashed the
  // instant CONFIRM is tapped -- survives `payload` being overwritten by
  // the /confirm response, and is what allocateThenRetry() replays as the
  // automatic retry after a successful top-up.
  const [lastAttempt, setLastAttempt] = useState(null);

  const confirm = async () => {
    setPhase('confirming');
    const attempt = { operator: payload.operator, params: payload.params, description: payload.description };
    setLastAttempt(attempt);
    try {
      const summary = describeOutcome(attempt);
      const data = await api.post('/orchie/capture/confirm', {
        sustain_id: sustainId, operator: attempt.operator, params: attempt.params,
        message_id: messageId, description: attempt.description,
      });
      setPayload(data);
      if (data.result.status === 'ok') {
        setCommittedSummary(summary);
        setPhase('committed');
        onCommitted?.();
      } else {
        setPhase('refused');
      }
    } catch (e) {
      setPayload({ why: e.message || 'could not reach orchie' });
      setPhase('error');
    }
  };

  // ── Allocate-then-retry recovery loop ──────────────────────────────────
  // A spend refused for INSUFFICIENT POCKET BALANCE (constraint_violated
  // === 'pocket_balance_sufficient', now carrying structured
  // {pocket, remaining, requested, shortfall} data -- see budget.spend's
  // own fail branch) is recoverable right here. The SOURCE the top-up
  // draws from is a real human CHOICE, never auto-picked (Bonnie's
  // explicit correction) -- the amount autofills from the shortfall, but
  // the person taps exactly which pocket (or the unallocated liquid pool)
  // it comes from. Two real gated operators, chosen by that tap:
  //   liquid source   -> budget.allocate(pocket_name, amount)
  //   another pocket  -> budget.transfer(from_pocket, to_pocket, amount)
  // (budget.transfer already existed -- no new operator was needed for
  // the pocket-to-pocket case.) Either way, a successful move is followed
  // by an automatic replay of the identical original spend. All three
  // calls go through the same /orchie/capture/confirm -> execute_operator()
  // path as every other write in this app -- no hand-written mutation, no
  // bypass of the gate any one of them would normally go through alone.
  const LIQUID_SOURCE = '__liquid__';
  const [allocateAmountOverride, setAllocateAmountOverride] = useState(null);
  const [allocateError, setAllocateError] = useState(null);
  const [sources, setSources] = useState(null); // {pockets: [{name,allocated,spent,available}], liquid_balance}
  const [sourcesLoading, setSourcesLoading] = useState(false);
  const shortfallData = payload?.result?.data;
  const isInsufficientBalance = payload?.result?.constraint_violated === 'pocket_balance_sufficient';

  // A fresh refusal (a different shortfall figure) clears any prior typed
  // override / stale error, and re-fetches source balances -- they may
  // have moved since the last time this card was shown.
  useEffect(() => {
    setAllocateAmountOverride(null);
    setAllocateError(null);
    setSources(null);
    if (!isInsufficientBalance) return;
    let cancelled = false;
    setSourcesLoading(true);
    api.get(`/orchie/capture/sources?sustain_id=${encodeURIComponent(sustainId)}`)
      .then(data => { if (!cancelled) setSources(data); })
      .catch(e => { if (!cancelled) setAllocateError(e.message || 'could not load pocket balances'); })
      .finally(() => { if (!cancelled) setSourcesLoading(false); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shortfallData?.shortfall, shortfallData?.pocket, isInsufficientBalance]);

  const allocateThenRetry = async (source) => {
    const pocketName = shortfallData?.pocket || lastAttempt?.params?.pocket_name;
    const amount = Number(allocateAmountOverride ?? shortfallData?.shortfall ?? 0);
    if (!pocketName || !(amount > 0)) {
      setAllocateError('enter a valid amount');
      return;
    }
    setAllocateError(null);
    setPhase(source === LIQUID_SOURCE ? 'allocating' : 'transferring');
    try {
      const moveResp = source === LIQUID_SOURCE
        ? await api.post('/orchie/capture/confirm', {
            sustain_id: sustainId, operator: 'budget.allocate', params: { pocket_name: pocketName, amount },
          })
        : await api.post('/orchie/capture/confirm', {
            sustain_id: sustainId, operator: 'budget.transfer',
            params: { from_pocket: source, to_pocket: pocketName, amount },
          });
      if (moveResp.result.status !== 'ok') {
        // Honest failure -- e.g. the chosen pocket doesn't actually have
        // that much spare allocation, or liquid can't cover it. Stay on
        // the refused view (payload untouched, so the picker is still
        // there) with the real reason shown -- pick a different source.
        setAllocateError(moveResp.result.reason || 'could not move funds');
        setPhase('refused');
        return;
      }
      setPhase('retrying');
      const retryResp = await api.post('/orchie/capture/confirm', {
        sustain_id: sustainId, operator: lastAttempt.operator, params: lastAttempt.params,
        message_id: messageId, description: lastAttempt.description,
      });
      setPayload(retryResp);
      if (retryResp.result.status === 'ok') {
        setCommittedSummary(describeOutcome(lastAttempt));
        setPhase('committed');
        onCommitted?.();
      } else {
        setPhase('refused');
      }
    } catch (e) {
      setAllocateError(e.message || 'could not reach orchie');
      setPhase('refused');
    }
  };

  // Auto-resolve on success: the card is never left ambiguously sitting
  // there once its job is done -- shows the confirmation, then collapses
  // on its own shortly after (CLOSE is still available for an immediate
  // manual dismiss). Never applies to 'refused' -- a refusal must stay
  // visible until the person retries or explicitly closes it.
  useEffect(() => {
    if (phase !== 'committed') return;
    const t = setTimeout(() => onClose?.(), 2600);
    return () => clearTimeout(t);
  }, [phase, onClose]);

  return (
    <div style={{
      marginTop: 12, padding: 14, borderRadius: 'var(--radius-md)',
      background: 'var(--bg-raised)', border: '1px solid var(--border-mid)',
    }}>
      {phase === 'loading' && <div style={mutedText}>thinking…</div>}

      {phase === 'needs_disambiguation' && payload.field === 'amount' && (
        <>
          <AmountEntry question={payload.question} why={payload.why} onSubmit={answer} />
          {messageId && (
            <NotATransactionControl
              sustainId={sustainId} messageId={messageId}
              onMarked={() => { onCommitted?.(); onClose?.(); }}
            />
          )}
        </>
      )}

      {phase === 'needs_disambiguation' && payload.field === 'pocket_name' && (
        <>
          <PocketPicker
            question={payload.question} why={payload.why} options={payload.options}
            sustainId={sustainId} onPick={answer}
          />
          {messageId && (
            <NotATransactionControl
              sustainId={sustainId} messageId={messageId}
              onMarked={() => { onCommitted?.(); onClose?.(); }}
            />
          )}
        </>
      )}

      {phase === 'needs_disambiguation' && payload.field !== 'amount' && payload.field !== 'pocket_name' && (
        <div>
          <div style={questionText}>{payload.question}</div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 12 }}>
            {payload.options.map(o => (
              <button key={o.value} onClick={() => answer(o.value)} style={bigTapButton}>{o.label}</button>
            ))}
          </div>
          <div style={{ ...mutedText, marginTop: 10 }}>{payload.why}</div>
          {messageId && (
            <NotATransactionControl
              sustainId={sustainId} messageId={messageId}
              onMarked={() => { onCommitted?.(); onClose?.(); }}
            />
          )}
        </div>
      )}

      {phase === 'ready' && (
        <ReadyReview
          payload={payload}
          onSetAmount={v => setKnownField('amount', v)}
          onSetDescription={v => setKnownField('description', v)}
          onSetDirection={setDirection}
          onChangePocket={changePocket}
          onChangePocketFromHistory={changePocketFromHistory}
          onConfirm={confirm}
          onCancel={onClose}
        />
      )}

      {phase === 'confirming' && (
        <div className="pulse" style={{
          display: 'flex', alignItems: 'center', gap: 8,
          fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 500, color: 'var(--text-primary)',
        }}>
          <span style={{ width: 8, height: 8, borderRadius: '50%', background: 'var(--amber)' }} />
          recording…
        </div>
      )}

      {phase === 'allocating' && (
        <div className="pulse" style={{
          display: 'flex', alignItems: 'center', gap: 8,
          fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 500, color: 'var(--text-primary)',
        }}>
          <span style={{ width: 8, height: 8, borderRadius: '50%', background: 'var(--amber)' }} />
          allocating…
        </div>
      )}

      {phase === 'transferring' && (
        <div className="pulse" style={{
          display: 'flex', alignItems: 'center', gap: 8,
          fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 500, color: 'var(--text-primary)',
        }}>
          <span style={{ width: 8, height: 8, borderRadius: '50%', background: 'var(--amber)' }} />
          transferring…
        </div>
      )}

      {phase === 'retrying' && (
        <div className="pulse" style={{
          display: 'flex', alignItems: 'center', gap: 8,
          fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 500, color: 'var(--text-primary)',
        }}>
          <span style={{ width: 8, height: 8, borderRadius: '50%', background: 'var(--amber)' }} />
          retrying…
        </div>
      )}

      {phase === 'committed' && (
        <div>
          <div style={{
            display: 'flex', alignItems: 'center', gap: 8,
            fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 700, color: 'var(--teal)',
          }}>
            <span style={{ width: 8, height: 8, borderRadius: '50%', background: 'var(--teal)' }} />
            ✓ recorded{committedSummary ? ` — ${committedSummary}` : ''}
          </div>
          <button onClick={onClose} style={{ ...cancelButton, marginTop: 10 }}>CLOSE</button>
        </div>
      )}

      {phase === 'refused' && (
        <div>
          <div style={{
            display: 'flex', alignItems: 'flex-start', gap: 8,
            fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 700, color: 'var(--danger)',
          }}>
            <span style={{ width: 8, height: 8, borderRadius: '50%', background: 'var(--danger)', marginTop: 4, flexShrink: 0 }} />
            <span>✗ not recorded — {payload.result?.reason || 'the gate refused this — nothing changed'}</span>
          </div>

          {isInsufficientBalance && (
            <div style={{
              marginTop: 10, padding: 10, borderRadius: 'var(--radius-sm)',
              background: 'var(--bg-overlay)', border: '1px solid var(--border-mid)',
            }}>
              <div style={mutedText}>
                allocate KES {allocateAmountOverride ?? Math.round(shortfallData?.shortfall ?? 0)} to '{shortfallData?.pocket}' — pick where it comes from
              </div>
              <div style={{ display: 'flex', gap: 8, marginTop: 8, alignItems: 'center' }}>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-secondary)' }}>KES</span>
                <input
                  type="number"
                  inputMode="decimal"
                  value={allocateAmountOverride ?? String(shortfallData?.shortfall ?? '')}
                  onChange={e => setAllocateAmountOverride(e.target.value)}
                  style={{
                    width: 90, fontFamily: 'var(--mono)', fontSize: 14, color: 'var(--text-primary)',
                    background: 'var(--bg-surface)', border: '1px solid var(--border-mid)',
                    borderRadius: 'var(--radius-sm)', padding: '8px 10px',
                  }}
                />
              </div>

              {sourcesLoading && <div style={{ ...mutedText, marginTop: 10 }}>loading pocket balances…</div>}

              {sources && (
                <div style={{ marginTop: 10 }}>
                  <div style={{ ...mutedText, marginBottom: 6 }}>from:</div>
                  <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
                    <button onClick={() => allocateThenRetry(LIQUID_SOURCE)} style={bigTapButton}>
                      unallocated · KES {Math.round(sources.liquid_balance).toLocaleString()}
                    </button>
                    {sources.pockets
                      .filter(p => p.name !== shortfallData?.pocket)
                      .sort((a, b) => b.available - a.available)
                      .map(p => (
                        <button key={p.name} onClick={() => allocateThenRetry(p.name)} style={bigTapButton}>
                          {p.name} · KES {Math.round(p.available).toLocaleString()}
                        </button>
                      ))}
                  </div>
                </div>
              )}

              {allocateError && (
                <div style={{ ...mutedText, color: 'var(--danger)', marginTop: 8 }}>{allocateError}</div>
              )}
            </div>
          )}

          <div style={{ display: 'flex', gap: 8, marginTop: 10 }}>
            <button onClick={() => runInfer(known)} style={confirmButton}>RETRY</button>
            <button onClick={onClose} style={cancelButton}>CLOSE</button>
          </div>
        </div>
      )}

      {phase === 'cannot_infer' && (
        <div>
          <div style={mutedText}>{payload.why}</div>
          <button onClick={onClose} style={{ ...cancelButton, marginTop: 10 }}>CLOSE</button>
        </div>
      )}

      {phase === 'error' && (
        <div style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--danger)' }}>{payload.why}</div>
      )}
    </div>
  );
}

function NarrateBar({ sustainId, onCommitted }) {
  const [text, setText] = useState('');
  const [active, setActive] = useState(false);

  if (active) {
    return (
      <div style={{ marginBottom: 14 }}>
        <div style={{ ...mutedText, marginBottom: 4 }}>capturing: “{text}”</div>
        <CaptureFlow
          sustainId={sustainId}
          effectText={text}
          onClose={() => { setActive(false); setText(''); }}
          onCommitted={() => { setActive(false); setText(''); onCommitted?.(); }}
        />
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', gap: 8, marginBottom: 14 }}>
      <input
        value={text}
        onChange={e => setText(e.target.value)}
        onKeyDown={e => { if (e.key === 'Enter' && text.trim()) setActive(true); }}
        placeholder="what happened? e.g. spent 500 on WiFi"
        style={{
          flex: 1, fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)',
          background: 'var(--bg-surface)', border: '1px solid var(--border-mid)',
          borderRadius: 'var(--radius-sm)', padding: '10px 12px',
        }}
      />
      <button
        onClick={() => text.trim() && setActive(true)}
        style={{ ...confirmButton, padding: '10px 16px' }}
      >
        GO
      </button>
    </div>
  );
}

// Module-level mount counter -- a hard, structural guarantee that the
// sync/window-picker card renders EXACTLY ONCE regardless of how many
// SmsCaptureCard elements end up in the tree at once (2 Aug 2026: Bonnie
// reported 5+ stacked copies of the sync card, with taps landing
// unpredictably -- exactly the symptom of several independent instances
// each tracking their own local `status`, so a tap on one card's "14
// days" button did nothing VISIBLE if a different stacked instance was
// the one Bonnie was actually looking at). Rather than resolve every
// possible cause of extra mounts (a WebView resume quirk, a stray
// duplicate render path, anything not reproducible from here), this
// makes duplication structurally impossible: only the FIRST instance to
// mount ever renders real content; any later one renders null and never
// touches SMS permissions, live polling, or the sync UI at all.
let _smsCaptureCardMountCount = 0;

/**
 * The in-app rationale + control surface for the native SMS auto-reader
 * (Android/Capacitor only — Tauri desktop has no SMS to read). Shows a
 * plain-language explanation BEFORE ever triggering Android's real
 * permission dialog (never on cold launch, per the standing rule for
 * "dangerous" runtime permissions) — see smsCapture.js's own header
 * comment for the full architecture (native captures + queues, this
 * module is the only thing that ever calls the ingest API).
 */
function SmsCaptureCard({ sustainId }) {
  // 'syncing' REMOVED from this state machine (2 Aug 2026 incident fix) --
  // it used to be an auto-firing intermediate status that triggered a
  // fixed 90-day backfill on every mount once permission was already
  // granted. Permission granted -> 'active' directly, always, with no
  // historical scan of any kind. The ONLY thing that ever reads inbox
  // history is a real tap inside WindowPicker, rendered as part of the
  // 'active' view below -- there is no other code path into backfillInbox().
  const [status, setStatus] = useState('checking'); // checking | prompt | requesting | active | denied | unavailable
  const [isPrimary, setIsPrimary] = useState(true);

  useEffect(() => {
    _smsCaptureCardMountCount += 1;
    // Every instance checks the CURRENT count the instant it mounts --
    // the first one to run this effect claims primacy (count is 1 for
    // it), any later one sees a higher count and stands down for its
    // whole lifetime, even if the earlier "primary" instance later
    // unmounts (no fighting over the role mid-session).
    if (_smsCaptureCardMountCount > 1) setIsPrimary(false);
    return () => { _smsCaptureCardMountCount -= 1; };
  }, []);

  useEffect(() => {
    // Non-primary instances (see _smsCaptureCardMountCount above) never
    // check permissions, never touch native SMS state -- not just a
    // render-output guard, a real no-op for the whole component.
    if (!isPrimary) return;
    if (!Capacitor.isNativePlatform()) { setStatus('unavailable'); return; }
    let cancelled = false;
    checkSmsPermission().then(perm => {
      if (cancelled) return;
      if (perm === 'granted') {
        setStatus('active');
      } else if (perm === 'denied') {
        setStatus('denied');
      } else {
        setStatus('prompt'); // 'prompt' or 'prompt-with-rationale'
      }
    }).catch(() => { if (!cancelled) setStatus('unavailable'); });
    return () => { cancelled = true; };
  }, [isPrimary]);

  useEffect(() => {
    // Real-time capture only (SmsReceiver's own queue, drained here) --
    // NOT a historical inbox scan. Starts the moment permission is
    // already granted; carries no flood risk since it only ever contains
    // messages that arrived after the receiver started listening.
    //
    // isPrimary-gated for the same reason as the effect above -- without
    // this, a non-primary instance would still call the module-level
    // startLivePolling() singleton in smsCapture.js, and its OWN
    // unmount could stop polling out from under the real primary
    // instance (stopLivePolling() has no notion of "whose" interval it's
    // clearing).
    if (!isPrimary || status !== 'active' || !sustainId) return;
    startLivePolling(sustainId);
    // Best-effort, silent -- a denied/unavailable notification permission
    // just means no nudge fires; the item still lands in the compose feed.
    ensureNotificationPermission().catch(() => {});
    return () => stopLivePolling();
  }, [isPrimary, status, sustainId]);

  const enable = async () => {
    setStatus('requesting');
    try {
      const perm = await requestSmsPermission();
      setStatus(perm === 'granted' ? 'active' : 'denied');
    } catch {
      setStatus('denied');
    }
  };

  if (!isPrimary) return null;
  if (status === 'unavailable' || status === 'checking') return null;

  if (status === 'active') {
    return (
      <div style={{ marginBottom: 14 }}>
        <div style={{
          display: 'flex', alignItems: 'center', gap: 6,
          fontFamily: 'var(--mono)', fontSize: 9.5, fontWeight: 500, color: 'var(--text-muted)',
        }}>
          <span style={{ width: 6, height: 6, borderRadius: '50%', background: 'var(--teal)' }} />
          real-time capture active — M-Pesa &amp; KCB
        </div>
        <WindowPicker sustainId={sustainId} />
      </div>
    );
  }

  return (
    <div style={{
      marginBottom: 14, padding: '14px 16px', borderRadius: 'var(--radius-md)',
      background: 'var(--bg-raised)', border: '1px solid var(--border-mid)',
    }}>
      <div style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)', marginBottom: 6 }}>
        Let Orchie read your M-Pesa &amp; KCB texts
      </div>
      <div style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.5, marginBottom: 12 }}>
        Orchie can pick up M-Pesa and KCB transaction alerts so your pockets stay current
        without typing anything in. It only ever reads messages from those two senders — every
        other text on your phone is never touched, never sent anywhere. New messages are
        captured as they arrive; nothing from your existing message history is read until you
        choose to sync it yourself. When something needs a quick decision, Orchie will send a
        notification so you can handle it right there in the field.
      </div>
      {status === 'requesting' && <div style={mutedText}>waiting for Android's permission dialog…</div>}
      {(status === 'prompt' || status === 'denied') && (
        <>
          <button onClick={enable} style={confirmButton}>ENABLE AUTOMATIC CAPTURE</button>
          {status === 'denied' && (
            <div style={{ ...mutedText, marginTop: 8 }}>
              Permission was declined — you can turn it on later from Android's app settings
              (Settings → Apps → Sustena Orchie → Permissions → SMS).
            </div>
          )}
        </>
      )}
    </div>
  );
}

/**
 * The ONLY way any historical inbox scan ever runs (2 Aug 2026, rebuilt
 * after an incident -- see backfillInbox()'s own docstring in
 * smsCapture.js for the full root cause). Always visible, never
 * collapsed behind a toggle -- per the explicit fix requirement, the
 * window choice is shown FIRST, before anything syncs, every time.
 *
 * Real safety properties, not just UI framing:
 *   - Nothing runs on render. A window button must be tapped.
 *   - "all" is not a single tap -- it shows an inline warning and needs a
 *     second, explicit CONFIRM tap. Every other window runs on one tap
 *     (7 days is listed first as the recommended default, per the
 *     incident's own lesson: never let the largest option be the
 *     easiest one to hit by accident).
 *   - STOP is real, not decorative: an AbortController's signal is
 *     threaded into backfillInbox() -> forwardMessages(), which checks
 *     it before EVERY network POST, so tapping STOP halts new requests
 *     immediately rather than draining whatever's left of the batch.
 *     The native inbox read itself (SmsCapturePlugin.readInbox) is a
 *     single fast local ContentProvider query, not itself interruptible
 *     from JS -- STOP's real effect is on the POST loop, which is what
 *     actually floods the server; this is disclosed here rather than
 *     silently overclaiming a native-level abort that isn't built.
 */
function WindowPicker({ sustainId }) {
  const [status, setStatus] = useState('idle'); // idle | confirming_all | syncing | done | error
  const [lastWindowLabel, setLastWindowLabel] = useState(null);
  const [result, setResult] = useState(null); // {new, duplicate, total}
  const [error, setError] = useState(null);
  const controllerRef = useRef(null);

  const runSync = async (w) => {
    const controller = new AbortController();
    controllerRef.current = controller;
    setLastWindowLabel(w.label);
    setStatus('syncing');
    setError(null);
    try {
      const r = await backfillInbox(sustainId, w.days, { signal: controller.signal });
      setResult(r);
      setStatus(controller.signal.aborted ? 'idle' : 'done');
    } catch (e) {
      setError(e.message || 'could not reach orchie');
      setStatus('error');
    } finally {
      controllerRef.current = null;
    }
  };

  const stop = () => {
    controllerRef.current?.abort();
    setStatus('idle');
  };

  const pick = (w) => {
    if (w.days <= 0) { setStatus('confirming_all'); return; }
    runSync(w);
  };

  // Auto-return to idle on a genuine success -- the result is shown
  // first, briefly, never silently swallowed, same discipline as
  // CaptureFlow's own 'committed' phase. Returning to 'idle' (the full
  // window row, not a collapsed state) is deliberate: it's what makes
  // "sync 7, then widen to 14, then 30" a real, immediately-repeatable
  // flow rather than something needing a page reload between taps.
  useEffect(() => {
    if (status !== 'done') return;
    const t = setTimeout(() => setStatus('idle'), 3200);
    return () => clearTimeout(t);
  }, [status]);

  return (
    <div style={{
      marginTop: 8, padding: 10, borderRadius: 'var(--radius-sm)',
      background: 'var(--bg-overlay)', border: '1px solid var(--border-mid)',
    }}>
      {status === 'idle' && (
        <>
          <div style={{ ...mutedText, marginBottom: 8 }}>
            sync M-Pesa &amp; KCB messages from the last:
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
            {RESYNC_WINDOWS.map(w => (
              <button key={w.label} onClick={() => pick(w)} style={bigTapButton}>{w.label}</button>
            ))}
          </div>
          {result && (
            <div style={{ ...mutedText, marginTop: 8 }}>
              last: {lastWindowLabel} — {result.new} new, {result.duplicate} already had. widen the window
              above to reach further back — already-synced messages are never double-captured.
            </div>
          )}
        </>
      )}

      {status === 'confirming_all' && (
        <div>
          <div style={{ ...mutedText, color: 'var(--amber)', marginBottom: 8 }}>
            "all" has no time limit — on a phone with years of M-Pesa/KCB history this could be
            thousands of messages. Sure?
          </div>
          <div style={{ display: 'flex', gap: 8 }}>
            <button onClick={() => runSync({ label: 'all', days: 0 })} style={confirmButton}>YES, SYNC ALL</button>
            <button onClick={() => setStatus('idle')} style={cancelButton}>CANCEL</button>
          </div>
        </div>
      )}

      {status === 'syncing' && (
        <div>
          <div className="pulse" style={{
            display: 'flex', alignItems: 'center', gap: 8,
            fontFamily: 'var(--mono)', fontSize: 12, fontWeight: 500, color: 'var(--text-primary)',
          }}>
            <span style={{ width: 8, height: 8, borderRadius: '50%', background: 'var(--amber)' }} />
            syncing {lastWindowLabel}…
          </div>
          <button onClick={stop} style={{ ...cancelButton, marginTop: 10, borderColor: 'var(--danger)', color: 'var(--danger)' }}>
            STOP
          </button>
        </div>
      )}

      {status === 'done' && (
        <div style={{
          display: 'flex', alignItems: 'center', gap: 8,
          fontFamily: 'var(--mono)', fontSize: 12, fontWeight: 700, color: 'var(--teal)',
        }}>
          <span style={{ width: 8, height: 8, borderRadius: '50%', background: 'var(--teal)' }} />
          ✓ {result?.new ?? 0} new captured ({result?.duplicate ?? 0} already had)
        </div>
      )}

      {status === 'error' && (
        <div>
          <div style={{ ...mutedText, color: 'var(--danger)' }}>{error}</div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 8 }}>
            {RESYNC_WINDOWS.map(w => (
              <button key={w.label} onClick={() => pick(w)} style={bigTapButton}>{w.label}</button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * General pocket management -- the on-ramp a freshly reset (or brand new)
 * sustain needs (Bonnie, 2 Aug 2026): without this, a zero-pocket sustain
 * had NO way to build a budget back up except stumbling into a classify
 * card's own "+ NEW" tile, which only appears mid-capture. This is the
 * standalone version -- always reachable from the top of Orchie, not
 * gated behind having a transaction to classify first.
 *
 * Reuses GET /orchie/capture/sources (built for the allocate-recovery
 * source picker) for the real, current pocket list + balances -- no new
 * read route needed. Pocket creation goes through the same real gated
 * budget.add_pocket every other pocket-creation path in this app uses.
 */
function PocketsPanel({ sustainId, onChanged }) {
  const [sources, setSources] = useState(null); // {pockets, liquid_balance}
  const [open, setOpen] = useState(false);
  const [everOpened, setEverOpened] = useState(false);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState('');
  const [status, setStatus] = useState('idle'); // idle | saving | error
  const [error, setError] = useState('');

  const reload = useCallback(async () => {
    if (!sustainId) return;
    try {
      const data = await api.get(`/orchie/capture/sources?sustain_id=${encodeURIComponent(sustainId)}`);
      setSources(data);
      // Auto-open once, the first time we learn there's nothing here yet --
      // the whole point is that an empty budget shouldn't need discovering.
      // Never re-forces itself open after that (e.g. a later reset), so a
      // person who deliberately collapsed it isn't fought with every load.
      if (data.pockets.length === 0 && !everOpened) { setOpen(true); setEverOpened(true); }
    } catch {
      setSources(null);
    }
  }, [sustainId, everOpened]);

  useEffect(() => { reload(); }, [sustainId]); // eslint-disable-line react-hooks/exhaustive-deps

  const createPocket = async () => {
    if (!name.trim() || status === 'saving') return;
    setStatus('saving');
    setError('');
    try {
      const data = await api.post('/orchie/capture/confirm', {
        sustain_id: sustainId, operator: 'budget.add_pocket', params: { pocket_name: name.trim() },
      });
      if (data.result.status === 'ok') {
        setName('');
        setCreating(false);
        setStatus('idle');
        await reload();
        onChanged?.();
      } else {
        setStatus('error');
        setError(data.result.reason || 'could not create the pocket');
      }
    } catch (e) {
      setStatus('error');
      setError(e.message || 'could not reach orchie');
    }
  };

  if (!sources) return null;
  const count = sources.pockets.length;

  return (
    <div style={{ marginBottom: 14 }}>
      <button
        onClick={() => setOpen(o => !o)}
        style={{
          background: 'none', border: 'none', padding: 0, cursor: 'pointer',
          fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.05em',
          color: 'var(--text-muted)',
        }}
      >
        {open ? 'hide pockets' : `pockets · ${count}`}
      </button>

      {open && (
        <div style={{
          marginTop: 8, padding: 12, borderRadius: 'var(--radius-md)',
          background: 'var(--bg-raised)', border: '1px solid var(--border-mid)',
        }}>
          {count === 0 ? (
            <div style={{ ...mutedText, marginBottom: 10 }}>
              no pockets yet — this is where your budget lives. create one to get started.
            </div>
          ) : (
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginBottom: 10 }}>
              {sources.pockets.map(p => (
                <div
                  key={p.name}
                  style={{
                    fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-secondary)',
                    background: 'var(--bg-overlay)', border: '1px solid var(--border-mid)',
                    borderRadius: 'var(--radius-sm)', padding: '8px 12px',
                  }}
                >
                  {p.name} · KES {Math.round(p.available).toLocaleString()}
                </div>
              ))}
            </div>
          )}

          {!creating ? (
            <button
              onClick={() => setCreating(true)}
              style={{ ...bigTapButton, borderStyle: 'dashed', borderColor: 'var(--teal-border)', color: 'var(--teal)' }}
            >
              + NEW POCKET
            </button>
          ) : (
            <div style={{ display: 'flex', gap: 8 }}>
              <input
                type="text" value={name}
                onChange={e => setName(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') createPocket(); }}
                placeholder="pocket name, e.g. Holiday Fund"
                autoFocus
                disabled={status === 'saving'}
                style={editableInput}
              />
              <button onClick={createPocket} style={confirmButton} disabled={status === 'saving'}>
                {status === 'saving' ? 'creating…' : 'CREATE'}
              </button>
            </div>
          )}

          {status === 'error' && (
            <div style={{ ...mutedText, color: 'var(--danger)', marginTop: 8 }}>{error}</div>
          )}
        </div>
      )}
    </div>
  );
}

function WidgetCard({ widget, sustainId, onCommitted }) {
  const d = widget.data || {};
  // classify_card is the in-field classify/confirm surface -- per Bonnie's
  // design intent, this belongs on the phone with the least possible
  // friction, so it opens straight into the capture flow (big-tap options
  // or a pre-filled CONFIRM) rather than gating behind an extra tap.
  const [capturing, setCapturing] = useState(widget.render === 'classify_card');

  return (
    <div style={{
      background: 'var(--bg-surface)', border: '1px solid var(--border-mid)',
      borderRadius: 'var(--radius-lg)', padding: '16px 18px', marginBottom: 14,
    }}>
      <div style={{
        // Reclassified from --text-dim to --text-muted (2 Aug 2026
        // legibility pass) -- this names which widget the person is
        // looking at, real content, not decoration.
        fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500, letterSpacing: '0.08em',
        color: 'var(--text-muted)', marginBottom: 8, textTransform: 'uppercase',
      }}>
        {widget.id.replace(/_/g, ' ')}
      </div>

      <div style={{
        display: 'flex', alignItems: 'baseline', gap: 8,
        fontFamily: 'var(--ui)', fontSize: 15, fontWeight: 500, color: 'var(--text-primary)', marginBottom: 10,
      }}>
        {widget.render === 'classify_card' && <DirectionBadge direction={d.parsed_fields?.direction} />}
        <span>{d.headline || widget.id}</span>
      </div>

      {widget.render === 'pocket_watch_card' && (
        <div style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-secondary)', display: 'flex', gap: 16 }}>
          <span>spent <b style={{ color: 'var(--text-primary)' }}>{d.spent}</b></span>
          <span>of <b style={{ color: 'var(--text-primary)' }}>{d.allocated}</b></span>
          <span style={{ color: d.pct_spent >= 1 ? 'var(--danger)' : d.pct_spent >= 0.8 ? 'var(--warn)' : 'var(--text-muted)' }}>
            {Math.round((d.pct_spent || 0) * 100)}%
          </span>
        </div>
      )}

      {widget.render === 'classify_card' && (
        <div style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-secondary)' }}>
          <div>source: {d.source_id || 'unknown'}</div>
          {d.reason && <div style={{ marginTop: 4, color: 'var(--text-muted)' }}>{d.reason}</div>}
          {d.raw_payload && (
            <div style={{
              marginTop: 8, padding: '8px 10px', borderRadius: 'var(--radius-sm)',
              background: 'var(--bg-base)', border: '1px solid var(--border)',
              color: 'var(--text-muted)', fontSize: 11, lineHeight: 1.5,
              whiteSpace: 'pre-wrap', wordBreak: 'break-word',
            }}>
              {d.raw_payload}
            </div>
          )}
        </div>
      )}

      {widget.render === 'rollup_summary_card' && (
        <div>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-secondary)', display: 'flex', flexWrap: 'wrap', gap: 16 }}>
            <span>unallocated <b style={{ color: 'var(--text-primary)' }}>KES {Math.round(d.liquid_balance || 0).toLocaleString()}</b></span>
            {d.household_total !== undefined && (
              <span>
                household total <b style={{ color: 'var(--amber)' }}>KES {Math.round(d.household_total || 0).toLocaleString()}</b>
                {' '}({d.included_children || 0} linked)
              </span>
            )}
          </div>
          <BalanceProvenance sustainId={sustainId} />
        </div>
      )}

      {widget.render === 'classify_card' && !capturing && (
        <button onClick={() => setCapturing(true)} style={{ ...confirmButton, marginTop: 12 }}>
          CLASSIFY
        </button>
      )}
      {widget.render === 'classify_card' && capturing && (
        <CaptureFlow
          sustainId={sustainId}
          messageId={d.message_id}
          onClose={() => setCapturing(false)}
          // The real bug behind "no feedback after CONFIRM": this used to
          // ALSO call setCapturing(false) here, synchronously, in the same
          // tick as CaptureFlow's own setPhase('committed') -- React 18
          // batches both updates into one re-render, so CaptureFlow
          // unmounted before its own "success" message ever painted. Now
          // only the background data refresh happens immediately;
          // collapsing the card is entirely CaptureFlow's own call (its
          // CLOSE button, or its auto-dismiss timer on the committed
          // phase) so the success/refusal state is always actually seen.
          onCommitted={() => onCommitted?.()}
        />
      )}

      {widget.render !== 'classify_card' && widget.emits && widget.emits.length > 0 && (
        <div style={{
          marginTop: 10, fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)',
          letterSpacing: '0.04em',
        }}>
          action available: {widget.emits.join(', ')}
        </div>
      )}

      <WhyReveal widget={widget} />
    </div>
  );
}

/**
 * One line of the balance trail -- an income (+) or an allocate (-), the
 * only two events that ever move liquid balance (verified by grep across
 * the whole backend, not assumed -- see orchie.py's own
 * _liquid_provenance_for_one_sustain docstring). Tap reveals the raw
 * message behind it, when one exists.
 */
function BalanceProvenanceEntry({ entry }) {
  const [expanded, setExpanded] = useState(false);
  const isIncome = entry.amount >= 0;
  const amountAbs = Math.round(Math.abs(entry.amount)).toLocaleString();
  const dateLabel = entry.date
    ? new Date(entry.date).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
    : '';

  return (
    <div
      onClick={() => entry.raw_text && setExpanded(e => !e)}
      style={{ padding: '8px 0', borderBottom: '1px solid var(--border)', cursor: entry.raw_text ? 'pointer' : 'default' }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', gap: 10 }}>
        <div style={{ minWidth: 0 }}>
          <div style={{
            fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-primary)',
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 200,
          }}>
            {entry.description || (isIncome ? 'income' : 'allocated to a pocket')}
          </div>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)', marginTop: 2 }}>
            {dateLabel}{entry.source ? ` · ${entry.source}` : ''}
          </div>
        </div>
        <div style={{ textAlign: 'right', flexShrink: 0 }}>
          <div style={{
            fontFamily: 'var(--mono)', fontSize: 12, fontWeight: 700,
            color: isIncome ? 'var(--teal)' : 'var(--amber)',
          }}>
            {isIncome ? '+' : '−'} KES {amountAbs}
          </div>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', marginTop: 2 }}>
            → KES {Math.round(entry.running_balance).toLocaleString()}
          </div>
        </div>
      </div>

      {expanded && entry.raw_text && (
        <div style={{
          marginTop: 6, padding: 8, borderRadius: 'var(--radius-sm)',
          background: 'var(--bg-base)', border: '1px solid var(--border)',
          fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)',
          whiteSpace: 'pre-wrap', wordBreak: 'break-word', lineHeight: 1.5,
        }}>
          {entry.raw_text}
        </div>
      )}
    </div>
  );
}

/**
 * Drillable provenance for the household/liquid rollup card (Bonnie,
 * 2 Aug 2026: "I still can't tell where the balance number came from...
 * I want PROVENANCE"). Sourced from GET /orchie/balance-provenance --
 * every event that moved this sustain's own liquid balance, oldest
 * first, summing to exactly what the card shows, with an explicit
 * consistency line (this project's own real bug-finding discipline made
 * visible: if the trace and the live number ever disagreed, this would
 * say so, in red, not paper over it).
 */
function BalanceProvenance({ sustainId }) {
  const [open, setOpen] = useState(false);
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const d = await api.get(`/orchie/balance-provenance?sustain_id=${encodeURIComponent(sustainId)}`);
      setData(d);
    } catch (e) {
      setError(e.message || 'could not reach orchie');
    } finally {
      setLoading(false);
    }
  };

  const toggle = () => {
    const next = !open;
    setOpen(next);
    if (next && data === null) load();
  };

  return (
    <div style={{ marginTop: 10 }}>
      <button
        onClick={toggle}
        style={{
          background: 'none', border: 'none', padding: 0, cursor: 'pointer',
          fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.04em',
          color: 'var(--text-muted)', textDecoration: 'underline dotted',
        }}
      >
        {open ? 'hide trace' : 'trace this balance'}
      </button>

      {open && (
        <div style={{ marginTop: 8 }}>
          {loading && <div style={mutedText}>loading…</div>}
          {error && <div style={{ ...mutedText, color: 'var(--danger)' }}>{error}</div>}
          {data && (
            <div style={{
              maxHeight: 360, overflowY: 'auto', padding: '4px 12px',
              borderRadius: 'var(--radius-md)', border: '1px solid var(--border-mid)',
              background: 'var(--bg-overlay)',
            }}>
              <div style={{
                fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)', padding: '6px 0',
                textTransform: 'uppercase', letterSpacing: '0.05em',
              }}>
                liquid — starting KES {Math.round(data.own.starting_balance).toLocaleString()}
              </div>
              {data.own.entries.length === 0 ? (
                <div style={mutedText}>no events yet — the balance has never moved</div>
              ) : (
                data.own.entries.map((e, i) => <BalanceProvenanceEntry key={i} entry={e} />)
              )}
              <div style={{
                marginTop: 8, padding: '8px 0', borderTop: '1px solid var(--border-mid)',
                display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 700,
              }}>
                <span style={{ color: 'var(--text-primary)' }}>= KES {Math.round(data.own.computed_total).toLocaleString()}</span>
                <span style={{ color: data.own.consistent ? 'var(--teal)' : 'var(--danger)' }}>
                  {data.own.consistent
                    ? '✓ matches live balance'
                    : `⚠ live shows KES ${Math.round(data.own.live_balance).toLocaleString()}`}
                </span>
              </div>

              {data.children.length > 0 && (
                <>
                  <div style={{
                    fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)', padding: '10px 0 6px',
                    textTransform: 'uppercase', letterSpacing: '0.05em',
                  }}>
                    household — {data.children.length} linked
                  </div>
                  {data.children.map(c => (
                    <div
                      key={c.sustain_id}
                      style={{
                        display: 'flex', justifyContent: 'space-between', padding: '4px 0',
                        fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-secondary)',
                      }}
                    >
                      <span>{c.label || c.sustain_id.slice(0, 8)}</span>
                      <span>KES {Math.round(c.live_balance).toLocaleString()}{c.consistent ? '' : ' ⚠'}</span>
                    </div>
                  ))}
                  <div style={{
                    marginTop: 6, padding: '6px 0', borderTop: '1px solid var(--border-mid)',
                    display: 'flex', justifyContent: 'space-between',
                    fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 700,
                  }}>
                    <span style={{ color: 'var(--amber)' }}>= KES {Math.round(data.household_total_computed).toLocaleString()}</span>
                    <span style={{ color: data.household_consistent ? 'var(--teal)' : 'var(--danger)' }}>
                      {data.household_consistent ? '✓ matches' : '⚠ mismatch'}
                    </span>
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * The processed/activity list (Bonnie, 2 Aug 2026: "there is no UI for the
 * processed" -- Orchie only ever showed what still needed a decision plus
 * the rollup number, never what had actually already been recorded).
 *
 * One row per real, committed spend/income transaction, newest first,
 * sourced from GET /orchie/activity (the real S3 event log, not a guess).
 * Tap a row to reveal the raw message behind it, when one exists -- the
 * first, deliberately lightweight cut of a fuller message-audit log.
 */
function ActivityEntryRow({ entry }) {
  const [expanded, setExpanded] = useState(false);
  const amount = entry.amount != null ? Math.round(entry.amount).toLocaleString() : '—';
  const dateLabel = entry.date
    ? new Date(entry.date).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
    : '';
  const title = entry.description || entry.pocket || (entry.direction === 'in' ? 'money received' : 'transaction');

  return (
    <div
      onClick={() => setExpanded(e => !e)}
      style={{ padding: '10px 2px', borderBottom: '1px solid var(--border)', cursor: 'pointer' }}
    >
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0 }}>
          <DirectionBadge direction={entry.direction === 'in' ? 'received' : 'sent'} />
          <div style={{ minWidth: 0 }}>
            <div style={{
              fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)',
              overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 220,
            }}>
              {title}
            </div>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)', marginTop: 2 }}>
              {entry.pocket ? `${entry.pocket} · ` : ''}{dateLabel}{entry.source ? ` · ${entry.source}` : ''}
            </div>
          </div>
        </div>
        <div style={{
          fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 700, flexShrink: 0,
          color: entry.direction === 'in' ? 'var(--teal)' : 'var(--text-primary)',
        }}>
          KES {amount}
        </div>
      </div>

      {expanded && (
        <div style={{
          marginTop: 8, padding: 10, borderRadius: 'var(--radius-sm)',
          background: 'var(--bg-overlay)', border: '1px solid var(--border-mid)',
        }}>
          {entry.raw_text ? (
            <div style={{
              fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-secondary)',
              whiteSpace: 'pre-wrap', wordBreak: 'break-word', lineHeight: 1.5,
            }}>
              {entry.raw_text}
            </div>
          ) : (
            <div style={mutedText}>entered directly — no message behind this</div>
          )}
        </div>
      )}
    </div>
  );
}

function ActivityList({ sustainId }) {
  const [open, setOpen] = useState(false);
  const [entries, setEntries] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.get(`/orchie/activity?sustain_id=${encodeURIComponent(sustainId)}&limit=50`);
      setEntries(data.entries);
    } catch (e) {
      setError(e.message || 'could not reach orchie');
    } finally {
      setLoading(false);
    }
  };

  const toggle = () => {
    const next = !open;
    setOpen(next);
    if (next && entries === null) load();
  };

  return (
    <div style={{ marginTop: 20 }}>
      <button
        onClick={toggle}
        style={{
          background: 'none', border: 'none', padding: 0, cursor: 'pointer',
          fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.05em',
          color: 'var(--text-muted)',
        }}
      >
        {open ? 'hide processed' : 'processed / activity'}
      </button>

      {open && (
        <div style={{ marginTop: 8 }}>
          {loading && <div style={mutedText}>loading…</div>}
          {error && <div style={{ ...mutedText, color: 'var(--danger)' }}>{error}</div>}
          {entries && entries.length === 0 && (
            <div style={mutedText}>nothing recorded yet</div>
          )}
          {entries && entries.length > 0 && (
            <div style={{
              maxHeight: 400, overflowY: 'auto', padding: '4px 12px',
              borderRadius: 'var(--radius-md)', border: '1px solid var(--border-mid)',
              background: 'var(--bg-raised)',
            }}>
              {entries.map((e, i) => (
                <ActivityEntryRow key={e.message_id || `${e.event_name}-${e.date}-${i}`} entry={e} />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/* ─── Nested holons (Phase 2, 2 Aug 2026) — breadcrumb, children, roll-up ───
 * compose(r) already resolves per-sustain, so "navigate" is just re-picking
 * sustainId (reusing useSustainPicker's chooseSustain, same mechanism the
 * TopBar picker already uses) -- no new state machinery, per the canon's
 * own §4H.6 note ("no new state machinery -- the per-request composer
 * already does this"). */

function FundTransferForm({ sustainId, targetId, targetLabel, onDone }) {
  const [amount, setAmount] = useState('');
  const [status, setStatus] = useState('idle'); // idle | sending | done | error
  const [error, setError] = useState(null);

  const send = async () => {
    const value = parseFloat(amount);
    if (!value || value <= 0) { setError('enter an amount greater than zero'); return; }
    setStatus('sending'); setError(null);
    try {
      const data = await api.post('/orchie/capture/confirm', {
        sustain_id: sustainId, operator: 'holon.transfer',
        params: { to_sustain_id: targetId, amount: value, idempotency_key: `orchie-${sustainId}-${targetId}-${Date.now()}` },
      });
      if (data.result.status === 'ok') { setStatus('done'); setTimeout(() => onDone?.(), 1400); }
      else { setStatus('error'); setError(data.result.reason || 'transfer refused'); }
    } catch (e) { setStatus('error'); setError(e.message || 'could not reach orchie'); }
  };

  if (status === 'done') {
    return <div style={{ ...mutedText, color: 'var(--teal)', fontWeight: 700 }}>✓ moved Ksh {amount} — {targetLabel}</div>;
  }

  return (
    <div style={{ marginTop: 8, display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
      <input
        type="number" inputMode="decimal" placeholder="amount"
        value={amount} onChange={e => setAmount(e.target.value)}
        style={{ ...editableInput, width: 100 }}
      />
      <button onClick={send} disabled={status === 'sending'} style={{ ...confirmButton, opacity: status === 'sending' ? 0.6 : 1 }}>
        {status === 'sending' ? 'moving…' : `MOVE TO ${targetLabel.toUpperCase()}`}
      </button>
      {error && <div style={{ ...mutedText, color: 'var(--danger)', width: '100%' }}>{error}</div>}
    </div>
  );
}

function NewHolonForm({ sustainId, onCreated, onCancel }) {
  const [name, setName] = useState('');
  const [status, setStatus] = useState('idle');
  const [error, setError] = useState(null);

  const create = async () => {
    const trimmed = name.trim();
    if (!trimmed) { setError('name it first'); return; }
    setStatus('creating'); setError(null);
    try {
      const data = await api.post('/orchie/capture/confirm', {
        sustain_id: sustainId, operator: 'holon.create_child',
        params: { template: 'habitat', name: trimmed },
      });
      if (data.result.status === 'ok') { onCreated?.(data.result.data.child_sustain_id); }
      else { setStatus('error'); setError(data.result.reason || 'could not create this holon'); }
    } catch (e) { setStatus('error'); setError(e.message || 'could not reach orchie'); }
  };

  return (
    <div style={{ marginTop: 10, padding: 12, borderRadius: 'var(--radius-sm)', border: '1px dashed var(--border-mid)' }}>
      <div style={{ ...mutedText, marginBottom: 8 }}>new holon — e.g. a project, a shared fund</div>
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
        <input
          placeholder="name it — e.g. Project IO" value={name}
          onChange={e => setName(e.target.value)}
          style={{ ...editableInput, flex: 1, minWidth: 140 }}
        />
        <button onClick={create} disabled={status === 'creating'} style={{ ...confirmButton, opacity: status === 'creating' ? 0.6 : 1 }}>
          {status === 'creating' ? 'creating…' : 'CREATE'}
        </button>
        <button onClick={onCancel} style={cancelButton}>CANCEL</button>
      </div>
      {error && <div style={{ ...mutedText, color: 'var(--danger)', marginTop: 8 }}>{error}</div>}
    </div>
  );
}

function RollupCard({ rollup }) {
  const aggregates = rollup?.aggregates ? Object.entries(rollup.aggregates) : [];
  if (aggregates.length === 0) return null;
  return (
    <div style={{
      marginTop: 12, padding: 14, borderRadius: 'var(--radius-md)',
      background: 'var(--bg-raised)', border: '1px solid var(--border)',
    }}>
      <div style={{ fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.08em', color: 'var(--text-muted)', marginBottom: 8, textTransform: 'uppercase' }}>
        household roll-up
      </div>
      {aggregates.map(([id, agg]) => (
        <div key={id} style={{ marginBottom: 6 }}>
          <div style={{ ...questionText, fontWeight: 700 }}>
            {id.replace(/_/g, ' ')} — Ksh {Number(agg.value || 0).toLocaleString()}
          </div>
          <div style={mutedText}>
            {agg.op} over {agg.included.length} linked holon{agg.included.length === 1 ? '' : 's'}
            {agg.excluded.length > 0 && ` · ${agg.excluded.length} excluded — ${agg.excluded[0].reason}`}
          </div>
        </div>
      ))}
    </div>
  );
}

function HolonNav({ sustainId, onNavigate }) {
  const [parent, setParent] = useState(undefined); // undefined = loading, null = none
  const [children, setChildren] = useState([]);
  const [rollup, setRollup] = useState(null);
  const [showNewForm, setShowNewForm] = useState(false);
  const [fundingTarget, setFundingTarget] = useState(null); // {id, label} or null

  const load = useCallback(async () => {
    if (!sustainId) return;
    setShowNewForm(false);
    setFundingTarget(null);
    try {
      const [parentResp, childrenResp, rollupResp] = await Promise.all([
        api.get(`/devui/sustain/${encodeURIComponent(sustainId)}/parent`),
        api.get(`/devui/sustain/${encodeURIComponent(sustainId)}/children`),
        api.get(`/devui/sustain/${encodeURIComponent(sustainId)}/rollup`).catch(() => ({ data: { aggregates: {}, children: [] } })),
      ]);
      setParent(parentResp.data.parent);
      setChildren(childrenResp.data.children || []);
      setRollup(rollupResp.data);
    } catch (e) {
      // best-effort -- honest silence, not a blocking error, matches this
      // page's own established discipline for secondary panels.
      setParent(null);
      setChildren([]);
      setRollup(null);
    }
  }, [sustainId]);

  useEffect(() => { load(); }, [load]);

  if (!sustainId) return null;

  return (
    <div style={{ marginBottom: 14 }}>
      {parent && (
        <button
          onClick={() => onNavigate(parent.parent_sustain_id)}
          style={{
            background: 'none', border: 'none', cursor: 'pointer', padding: '4px 0 8px',
            fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-secondary)',
          }}
        >
          ← {parent.display_name || 'up'}
        </button>
      )}

      {children.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {children.map(c => (
            <div key={c.child_sustain_id} style={{
              display: 'flex', alignItems: 'center', justifyContent: 'space-between',
              padding: '10px 12px', borderRadius: 'var(--radius-sm)',
              background: 'var(--bg-overlay)', border: '1px solid var(--border-mid)',
            }}>
              <button
                onClick={() => onNavigate(c.child_sustain_id)}
                style={{ background: 'none', border: 'none', cursor: 'pointer', textAlign: 'left', flex: 1, ...questionText }}
              >
                {c.member || c.display_name || c.slot} →
              </button>
              <button
                onClick={() => setFundingTarget(t => (t?.id === c.child_sustain_id ? null : { id: c.child_sustain_id, label: c.member || c.slot }))}
                style={{ ...cancelButton, padding: '6px 10px', fontSize: 9 }}
              >
                FUND
              </button>
            </div>
          ))}
        </div>
      )}

      {fundingTarget && (
        <FundTransferForm
          sustainId={sustainId} targetId={fundingTarget.id} targetLabel={fundingTarget.label}
          onDone={() => load()}
        />
      )}

      <RollupCard rollup={rollup} />

      {!showNewForm ? (
        <button
          onClick={() => setShowNewForm(true)}
          style={{
            marginTop: 10, background: 'none', border: '1px dashed var(--border-mid)', borderRadius: 'var(--radius-sm)',
            padding: '10px 14px', cursor: 'pointer', width: '100%',
            fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.06em', color: 'var(--text-muted)',
          }}
        >
          + NEW HOLON
        </button>
      ) : (
        <NewHolonForm
          sustainId={sustainId}
          onCancel={() => setShowNewForm(false)}
          onCreated={() => load()}
        />
      )}
    </div>
  );
}

function Empty({ text }) {
  return (
    <div style={{
      padding: '40px 20px', textAlign: 'center', fontFamily: 'var(--ui)',
      fontSize: 13, color: 'var(--text-muted)',
    }}>
      {text}
    </div>
  );
}

export default function OrchieShell() {
  // Reactive (not a plain read) so LoginGate's onSignedIn can flip the app
  // straight into the compose view without a page reload -- the native app
  // starts every cold launch with no token at all, unlike the hosted web
  // app which usually already has a browser session by the time this
  // screen renders.
  const [token, setToken] = useState(() => (
    typeof window !== 'undefined' ? window.localStorage.getItem('sustena_token') : null
  ));
  const { sustainId, sustains, resolving, chooseSustain } = useSustainPicker(token);
  const [view, setView] = useState(null);
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(true);
  const [showExcluded, setShowExcluded] = useState(false);
  // A tap on the native real-time "Orchie needs a decision" notification --
  // jump straight to THAT item's classify/confirm card, independent of
  // whatever compose() would otherwise rank first.
  const [pendingTarget, setPendingTarget] = useState(null);

  const load = useCallback(async () => {
    if (!sustainId) return;
    setLoading(true);
    setError(null);
    try {
      const resp = await api.get(`/orchie/compose?sustain_id=${encodeURIComponent(sustainId)}&device=phone&budget=4`);
      setView(resp);
    } catch (e) {
      setError(e.message || 'orchie is unreachable right now');
    } finally {
      setLoading(false);
    }
  }, [sustainId]);

  useEffect(() => { load(); }, [load]);

  // Native code (IngestWorker.java, triggered directly by SmsReceiver on
  // real-time SMS arrival) can't read the WebView's localStorage -- push
  // the real token + active sustain into native storage whenever either
  // changes, so a real-time capture can be authenticated with no JS/app
  // process involved. Best-effort, silent; the JS poll/backfill path is
  // unaffected either way.
  useEffect(() => {
    setNativeAuthContext(token, sustainId).catch(() => {});
  }, [token, sustainId]);

  // Pick up a pending classify target: once on mount, and again whenever
  // the app becomes visible (a notification tap while already running
  // routes through Android's onNewIntent, not a fresh mount -- checking on
  // visibilitychange catches that case too).
  const checkPendingTarget = useCallback(async () => {
    const target = await consumePendingClassifyTarget();
    if (!target) return;
    if (target.sustainId !== sustainId) chooseSustain(target.sustainId);
    setPendingTarget(target);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sustainId]);

  useEffect(() => { checkPendingTarget(); }, [checkPendingTarget]);

  useEffect(() => {
    const onVisible = () => { if (document.visibilityState === 'visible') checkPendingTarget(); };
    document.addEventListener('visibilitychange', onVisible);
    return () => document.removeEventListener('visibilitychange', onVisible);
  }, [checkPendingTarget]);

  if (!token) {
    return (
      <div style={pageStyle}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', padding: '18px 4px 20px' }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 12, fontWeight: 700, letterSpacing: '0.1em', color: 'var(--text-primary)' }}>
            ORCHIE
          </span>
        </div>
        <LoginGate onSignedIn={() => setToken(window.localStorage.getItem('sustena_token'))} />
      </div>
    );
  }

  return (
    <div style={pageStyle}>
      <div style={{
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        padding: '18px 4px 20px',
      }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 12, fontWeight: 700, letterSpacing: '0.1em', color: 'var(--text-primary)' }}>
          ORCHIE
        </span>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <SustainPicker sustainId={sustainId} sustains={sustains} onChoose={chooseSustain} />
          <button
            onClick={load}
            style={{
              background: 'none', border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
              padding: '4px 10px', cursor: 'pointer',
              fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.06em', color: 'var(--text-muted)',
            }}
          >
            ↺ REFRESH
          </button>
        </div>
      </div>

      {pendingTarget && pendingTarget.sustainId === sustainId && (
        <div style={{
          marginBottom: 14, padding: 14, borderRadius: 'var(--radius-md)',
          background: 'var(--bg-raised)', border: '1px solid var(--teal-border)',
        }}>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.08em', color: 'var(--teal)', marginBottom: 4, textTransform: 'uppercase' }}>
            from notification
          </div>
          <CaptureFlow
            sustainId={sustainId}
            messageId={pendingTarget.messageId}
            onClose={() => setPendingTarget(null)}
            onCommitted={() => { setPendingTarget(null); load(); }}
          />
        </div>
      )}

      {sustainId && <HolonNav sustainId={sustainId} onNavigate={chooseSustain} />}

      {sustainId && <SmsCaptureCard key={sustainId} sustainId={sustainId} />}

      {sustainId && <PocketsPanel key={sustainId} sustainId={sustainId} onChanged={load} />}

      {sustainId && <NarrateBar sustainId={sustainId} onCommitted={load} />}

      {(resolving || loading) && <Empty text="composing…" />}

      {!resolving && !loading && !sustainId && (
        <Empty text="no sustain to show — create one in Mycelium first" />
      )}

      {error && <Empty text={error} />}

      {view && !error && (
        <>
          {(!view.selected || view.selected.length === 0) ? (
            <Empty text={view.message || 'nothing needs you right now'} />
          ) : (
            // key: multiple classify_card instances (one needs_attention
            // message each) all share the same widget.id
            // ("unmapped_capture_classify") -- compose() doesn't qualify
            // it per message, so w.id alone would collide and let React
            // conflate two different cards. Fall back to the real
            // message_id when present, which IS unique per card.
            view.selected.map(w => (
              <WidgetCard key={w.data?.message_id ? `${w.id}:${w.data.message_id}` : w.id} widget={w} sustainId={sustainId} onCommitted={load} />
            ))
          )}

          {view.excluded && view.excluded.length > 0 && (
            <div style={{ marginTop: 8, textAlign: 'center' }}>
              <button
                onClick={() => setShowExcluded(s => !s)}
                style={{
                  background: 'none', border: 'none', cursor: 'pointer',
                  // Reclassified from --text-dim to --text-muted (2 Aug
                  // 2026 legibility pass) -- a real, tappable control label.
                  fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, color: 'var(--text-muted)',
                }}
              >
                {showExcluded ? 'hide' : `${view.excluded.length} other thing${view.excluded.length === 1 ? '' : 's'} stayed quiet`}
              </button>
              {showExcluded && (
                <div style={{ marginTop: 8, fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)', textAlign: 'left' }}>
                  {view.excluded.map(e => (
                    <div key={e.id} style={{ padding: '4px 0', borderTop: '1px solid var(--border)' }}>
                      {e.id.replace(/_/g, ' ')} — score {e.score} — {e.why_excluded}
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </>
      )}

      {sustainId && <ActivityList sustainId={sustainId} />}
    </div>
  );
}

const pageStyle = {
  minHeight: '100vh',
  maxWidth: 480,
  margin: '0 auto',
  padding: '0 16px 40px',
  background: 'var(--bg-base)',
  color: 'var(--text-primary)',
};
