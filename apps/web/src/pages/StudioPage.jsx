/**
 * apps/web/src/pages/StudioPage.jsx
 *
 * THE SUSTENA MODELING STUDIO — Slice 1 walking skeleton.
 *
 * A new, desktop-first surface that replaces the scattered /devui panels'
 * "eight peer panels" information architecture with a SUSTAIN-CENTRIC one:
 * pick one Sustain, and the Studio presents that one system through five
 * verbs — Monitor / Control / Simulate / Network / Edit — all operating on
 * the same selected sustain, with a persistent visual representation of the
 * sustain itself always in frame above whichever verb is active. This is
 * the Capstone's six-layer frame (SUSTENA_UPGRADE_SPEC.md §9.1) read as an
 * information architecture: Definition (Edit), Cognition (Monitor,
 * Simulate), Governance (Control), and the composition layer (Network).
 *
 * Every zone below is wired to ENGINE endpoints that already exist —
 * nothing here reimplements SustainEngine logic. This slice does not touch
 * a single /devui route or Mycelium panel; it is additive, served
 * alongside the existing surface at "/".
 */
import React, { useEffect, useState, useCallback } from 'react';
import { Link } from 'react-router-dom';
import { api } from '../lib/api';
import SustainGraph from './studio/SustainGraph.jsx';
import MonitorZone from './studio/MonitorZone.jsx';
import ControlZone from './studio/ControlZone.jsx';
import SimulateZone from './studio/SimulateZone.jsx';
import NetworkZone from './studio/NetworkZone.jsx';
import EditZone from './studio/EditZone.jsx';
import { panelCard, mutedText, Empty } from './studio/shared.jsx';

const VERBS = [
  { key: 'monitor',  label: 'MONITOR',  sub: 'observe',  color: 'var(--node-state)' },
  { key: 'control',  label: 'CONTROL',  sub: 'execute',  color: 'var(--node-operator)' },
  { key: 'simulate', label: 'SIMULATE', sub: 'what-if',  color: 'var(--amber)' },
  { key: 'network',  label: 'NETWORK',  sub: 'compose',  color: 'var(--node-event)' },
  { key: 'edit',     label: 'EDIT',     sub: 'inspect',  color: 'var(--node-operative)' },
];

export default function StudioPage() {
  const token = typeof window !== 'undefined' ? localStorage.getItem('sustena_token') : null;

  const [sustains, setSustains] = useState([]);
  const [sustainId, setSustainId] = useState(null);
  const [loadingSustains, setLoadingSustains] = useState(true);

  const [definition, setDefinition] = useState(null);
  const [defLoading, setDefLoading] = useState(true);
  const [children, setChildren] = useState([]);

  const [verb, setVerb] = useState('monitor');

  const loadSustains = useCallback(async () => {
    setLoadingSustains(true);
    try {
      const resp = await api.get('/devui/sustains');
      const list = resp?.data?.sustains || [];
      setSustains(list);
      setSustainId(prev => prev || list[0]?.id || null);
    } catch {
      setSustains([]);
    } finally {
      setLoadingSustains(false);
    }
  }, []);

  useEffect(() => { if (token) loadSustains(); }, [token, loadSustains]);

  const loadDefinition = useCallback(async () => {
    if (!sustainId) return;
    setDefLoading(true);
    try {
      const [defResp, childResp] = await Promise.all([
        api.get(`/devui/sustain/${encodeURIComponent(sustainId)}/definition`),
        api.get(`/devui/sustain/${encodeURIComponent(sustainId)}/children`),
      ]);
      setDefinition(defResp?.data || null);
      setChildren(childResp?.data?.children || []);
    } catch {
      setDefinition(null);
      setChildren([]);
    } finally {
      setDefLoading(false);
    }
  }, [sustainId]);

  useEffect(() => { loadDefinition(); }, [loadDefinition]);

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

  const selected = sustains.find(s => s.id === sustainId);

  return (
    <div style={pageStyle}>
      {/* Header */}
      <div style={{
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        padding: '20px 28px', borderBottom: '1px solid var(--border)', flexShrink: 0,
      }}>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 14 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 14, fontWeight: 700, letterSpacing: '0.14em', color: 'var(--text-primary)' }}>
            STUDIO
          </span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.08em', color: 'var(--text-dim)' }}>
            the modeling workbench
          </span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
          {loadingSustains ? (
            <span style={mutedText}>loading sustains…</span>
          ) : sustains.length === 0 ? (
            <span style={mutedText}>no sustains yet</span>
          ) : (
            <select
              value={sustainId || ''}
              onChange={e => setSustainId(e.target.value)}
              style={{
                fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-primary)',
                background: 'var(--bg-surface)', border: '1px solid var(--border-mid)',
                borderRadius: 'var(--radius-sm)', padding: '7px 12px', minWidth: 200,
              }}
            >
              {sustains.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}
            </select>
          )}
          <span style={{
            display: 'inline-flex', alignItems: 'center', gap: 5,
            fontFamily: 'var(--mono)', fontSize: 9.5, color: 'var(--text-dim)',
          }}>
            <span style={{ width: 6, height: 6, borderRadius: '50%', background: 'var(--ok)' }} />
            LIVE
          </span>
          <Link to="/" style={{ fontFamily: 'var(--mono)', fontSize: 10.5, color: 'var(--text-muted)' }}>← mycelium</Link>
        </div>
      </div>

      {!sustainId ? (
        <Empty text="no sustain selected" sub="create one from Mycelium's TopBar selector first" />
      ) : (
        <div style={{ padding: '22px 28px 40px', maxWidth: 1180, margin: '0 auto', width: '100%' }}>
          {/* Persistent sustain graph — the "one coherent frame" */}
          <div style={{ ...panelCard, marginBottom: 22 }}>
            <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 14 }}>
              <div>
                <div style={{ fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 700, letterSpacing: '0.06em', color: 'var(--text-primary)' }}>
                  {selected?.label || sustainId}
                </div>
                <div style={{ ...mutedText, marginTop: 3 }}>
                  {definition?.description || 'the sustain — every engine below operates on this one system'}
                </div>
              </div>
            </div>
            <SustainGraph definition={definition} children={children} loading={defLoading} />
          </div>

          {/* Verb tabs */}
          <div style={{ display: 'flex', gap: 6, marginBottom: 16 }}>
            {VERBS.map(v => {
              const active = verb === v.key;
              return (
                <button
                  key={v.key}
                  onClick={() => setVerb(v.key)}
                  style={{
                    fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 700, letterSpacing: '0.06em',
                    color: active ? 'var(--bg-base)' : 'var(--text-secondary)',
                    background: active ? v.color : 'var(--bg-surface)',
                    border: `1px solid ${active ? v.color : 'var(--border-mid)'}`,
                    borderRadius: 'var(--radius-sm)', padding: '9px 16px', cursor: 'pointer',
                    display: 'flex', flexDirection: 'column', alignItems: 'flex-start', gap: 1,
                  }}
                >
                  <span>{v.label}</span>
                  <span style={{ fontSize: 8, fontWeight: 400, opacity: 0.75, letterSpacing: '0.04em' }}>{v.sub}</span>
                </button>
              );
            })}
          </div>

          {/* Active verb zone */}
          <div style={panelCard}>
            {verb === 'monitor' && <MonitorZone sustainId={sustainId} />}
            {verb === 'control' && <ControlZone sustainId={sustainId} />}
            {verb === 'simulate' && <SimulateZone sustainId={sustainId} />}
            {verb === 'network' && <NetworkZone sustainId={sustainId} definition={definition} />}
            {verb === 'edit' && <EditZone definition={definition} loading={defLoading} />}
          </div>
        </div>
      )}
    </div>
  );
}

const pageStyle = {
  minHeight: '100vh',
  background: 'var(--bg-base)',
  color: 'var(--text-primary)',
  display: 'flex',
  flexDirection: 'column',
};
