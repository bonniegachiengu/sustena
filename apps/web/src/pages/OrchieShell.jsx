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
 * Scope note (disclosed, not hidden): this slice is READ-ONLY composition.
 * Each card's declared `emits` (the operator it could trigger) is shown as
 * plain text, not a live button — effect-first capture (routing a tap or a
 * narrated effect through admit()) is the next slice, not this one.
 */
import React, { useEffect, useState, useCallback } from 'react';
import { useSearchParams, Link } from 'react-router-dom';
import { api } from '../lib/api';

function useSustainId() {
  const [searchParams] = useSearchParams();
  const explicit = searchParams.get('sustain');
  const [sustainId, setSustainId] = useState(explicit);
  const [resolving, setResolving] = useState(!explicit);

  useEffect(() => {
    if (explicit) { setSustainId(explicit); setResolving(false); return; }
    let cancelled = false;
    (async () => {
      try {
        const resp = await api.get('/devui/sustains');
        const list = resp?.data?.sustains || [];
        if (!cancelled) setSustainId(list[0]?.id || null);
      } catch {
        if (!cancelled) setSustainId(null);
      } finally {
        if (!cancelled) setResolving(false);
      }
    })();
    return () => { cancelled = true; };
  }, [explicit]);

  return { sustainId, resolving };
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

function WidgetCard({ widget }) {
  const d = widget.data || {};
  return (
    <div style={{
      background: 'var(--bg-surface)', border: '1px solid var(--border-mid)',
      borderRadius: 'var(--radius-lg)', padding: '16px 18px', marginBottom: 14,
    }}>
      <div style={{
        fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.08em',
        color: 'var(--text-dim)', marginBottom: 8, textTransform: 'uppercase',
      }}>
        {widget.id.replace(/_/g, ' ')}
      </div>

      <div style={{ fontFamily: 'var(--ui)', fontSize: 15, fontWeight: 500, color: 'var(--text-primary)', marginBottom: 10 }}>
        {d.headline || widget.id}
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

      {widget.emits && widget.emits.length > 0 && (
        <div style={{
          marginTop: 10, fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)',
          letterSpacing: '0.04em',
        }}>
          action available: {widget.emits.join(', ')} <span style={{ opacity: 0.6 }}>(not wired yet — next slice)</span>
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
  const { sustainId, resolving } = useSustainId();
  const [view, setView] = useState(null);
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(true);
  const [showExcluded, setShowExcluded] = useState(false);
  const token = typeof window !== 'undefined' ? localStorage.getItem('sustena_token') : null;

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

  if (!token) {
    return (
      <div style={pageStyle}>
        <Empty text="sign in required" />
        <div style={{ textAlign: 'center' }}>
          <Link to="/" style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--amber)' }}>go to sustena →</Link>
        </div>
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
            view.selected.map(w => <WidgetCard key={w.id} widget={w} />)
          )}

          {view.excluded && view.excluded.length > 0 && (
            <div style={{ marginTop: 8, textAlign: 'center' }}>
              <button
                onClick={() => setShowExcluded(s => !s)}
                style={{
                  background: 'none', border: 'none', cursor: 'pointer',
                  fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)',
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
