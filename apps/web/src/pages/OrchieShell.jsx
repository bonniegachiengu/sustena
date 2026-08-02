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
import React, { useEffect, useState, useCallback } from 'react';
import { useSearchParams, Link } from 'react-router-dom';
import { Capacitor } from '@capacitor/core';
import { api } from '../lib/api';
import LoginGate from '../components/LoginGate';
import {
  checkSmsPermission, requestSmsPermission, backfillInbox, startLivePolling, stopLivePolling,
  ensureNotificationPermission, setNativeAuthContext, consumePendingClassifyTarget,
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

  const confirm = async () => {
    setPhase('confirming');
    try {
      const data = await api.post('/orchie/capture/confirm', {
        sustain_id: sustainId, operator: payload.operator, params: payload.params,
        message_id: messageId, description: payload.description,
      });
      setPayload(data);
      if (data.result.status === 'ok') {
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

  return (
    <div style={{
      marginTop: 12, padding: 14, borderRadius: 'var(--radius-md)',
      background: 'var(--bg-raised)', border: '1px solid var(--border-mid)',
    }}>
      {phase === 'loading' && <div style={mutedText}>thinking…</div>}

      {phase === 'needs_disambiguation' && payload.field === 'amount' && (
        <AmountEntry question={payload.question} why={payload.why} onSubmit={answer} />
      )}

      {phase === 'needs_disambiguation' && payload.field !== 'amount' && (
        <div>
          <div style={questionText}>{payload.question}</div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 12 }}>
            {payload.options.map(o => (
              <button key={o.value} onClick={() => answer(o.value)} style={bigTapButton}>{o.label}</button>
            ))}
          </div>
          <div style={{ ...mutedText, marginTop: 10 }}>{payload.why}</div>
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

      {phase === 'confirming' && <div style={mutedText}>applying…</div>}

      {phase === 'committed' && (
        <div>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--teal)' }}>
            ✓ done — {payload.operator} applied, state updated
          </div>
          <button onClick={onClose} style={{ ...cancelButton, marginTop: 10 }}>CLOSE</button>
        </div>
      )}

      {phase === 'refused' && (
        <div>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--danger)' }}>
            ✗ refused — {payload.result?.reason || 'the gate refused this — nothing changed'}
          </div>
          <button onClick={onClose} style={{ ...cancelButton, marginTop: 10 }}>CLOSE</button>
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
  const [status, setStatus] = useState('checking'); // checking | prompt | requesting | syncing | active | denied | unavailable
  const [syncedCount, setSyncedCount] = useState(null);

  useEffect(() => {
    if (!Capacitor.isNativePlatform()) { setStatus('unavailable'); return; }
    let cancelled = false;
    checkSmsPermission().then(perm => {
      if (cancelled) return;
      if (perm === 'granted') {
        setStatus('syncing');
      } else if (perm === 'denied') {
        setStatus('denied');
      } else {
        setStatus('prompt'); // 'prompt' or 'prompt-with-rationale'
      }
    }).catch(() => { if (!cancelled) setStatus('unavailable'); });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (status !== 'syncing' || !sustainId) return;
    let cancelled = false;
    backfillInbox(sustainId).then(count => {
      if (cancelled) return;
      setSyncedCount(count);
      setStatus('active');
      startLivePolling(sustainId);
      // Best-effort, silent -- a denied/unavailable notification permission
      // just means no nudge fires; the item still lands in the compose feed.
      ensureNotificationPermission().catch(() => {});
    }).catch(() => { if (!cancelled) setStatus('active'); });
    return () => { cancelled = true; };
  }, [status, sustainId]);

  useEffect(() => {
    // Stop the poll if the component unmounts or the selected sustain
    // changes (a new mount will restart it against the new sustainId).
    return () => stopLivePolling();
  }, []);

  const enable = async () => {
    setStatus('requesting');
    try {
      const perm = await requestSmsPermission();
      setStatus(perm === 'granted' ? 'syncing' : 'denied');
    } catch {
      setStatus('denied');
    }
  };

  if (status === 'unavailable' || status === 'checking') return null;

  if (status === 'active') {
    return (
      <div style={{
        display: 'flex', alignItems: 'center', gap: 6, marginBottom: 14,
        // Reclassified from --text-dim to --text-muted (2 Aug 2026
        // legibility pass) -- a real status line, not decoration.
        fontFamily: 'var(--mono)', fontSize: 9.5, fontWeight: 500, color: 'var(--text-muted)',
      }}>
        <span style={{ width: 6, height: 6, borderRadius: '50%', background: 'var(--teal)' }} />
        auto-capture active — M-Pesa &amp; KCB{syncedCount != null ? ` · ${syncedCount} synced` : ''}
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
        Orchie can automatically pick up M-Pesa and KCB transaction alerts so your pockets
        stay current without typing anything in. It only ever reads messages from those two
        senders — every other text on your phone is never touched, never sent anywhere. When
        something needs a quick decision, Orchie will send a notification so you can handle
        it right there in the field.
      </div>
      {status === 'requesting' && <div style={mutedText}>waiting for Android's permission dialog…</div>}
      {status === 'syncing' && <div style={mutedText}>syncing recent messages…</div>}
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
        <div style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-secondary)', display: 'flex', gap: 16 }}>
          <span>liquid <b style={{ color: 'var(--text-primary)' }}>{d.liquid_balance}</b></span>
          {d.household_total !== undefined && (
            <span>household <b style={{ color: 'var(--amber)' }}>{d.household_total}</b> ({d.included_children} linked)</span>
          )}
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
          onCommitted={() => { setCapturing(false); onCommitted?.(); }}
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

      {sustainId && <SmsCaptureCard key={sustainId} sustainId={sustainId} />}

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
            view.selected.map(w => <WidgetCard key={w.id} widget={w} sustainId={sustainId} onCommitted={load} />)
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
