import React, { useState, useEffect, useRef } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { api } from '../lib/api.js';

/* ── Orchie Panel ─────────────────────────────────────────────────────────────
   Full-page panel opened from the Orchie FAB expand button.

   Data sources (all real — no hardcoded values):
     POST /devui/console/execute { operator: 'orchie.morning_brief' } → TODAY card
     GET  /devui/monitor-widgets?sustain_id=...                        → PINNED MONITORING
     GET  /devui/state?sustain_id=...                                  → proposals + tasks
     POST /orchie/message                                               → chat replies
     POST /devui/console/execute { operator: 'homestead.tasks.complete' } → task checkbox

   Empty states follow the Sustena design language from CLAUDE.md.
*/

/* ── Tiny primitives ─────────────────────────────────────────────────────────── */

function Dot({ color = 'var(--amber)', pulse }) {
  return (
    <span className={pulse ? 'pulse' : undefined} style={{
      width: 5, height: 5, borderRadius: '50%',
      background: color, flexShrink: 0, display: 'inline-block',
    }} />
  );
}

function SLabel({ children, style }) {
  return (
    <span style={{
      fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.1em',
      color: 'var(--text-dim)', textTransform: 'uppercase', ...style,
    }}>{children}</span>
  );
}

function Empty({ text }) {
  return (
    <span style={{
      fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)',
      letterSpacing: '0.04em',
    }}>{text}</span>
  );
}

function SustenaMark({ size = 18 }) {
  const endX = 78 * Math.cos((28 * Math.PI) / 180);
  const endY = 78 * Math.sin((28 * Math.PI) / 180);
  const bp = `M 0 0 Q 52 0 ${endX} ${endY}`;
  return (
    <svg width={size} height={size} viewBox="-100 -100 200 200" style={{ display: 'block' }}>
      {[-90, 30, 150].map(r => (
        <g key={r} transform={`rotate(${r})`}>
          <path d={bp} stroke="var(--text-primary)" strokeWidth="14" fill="none" strokeLinecap="round" />
          <circle cx={endX} cy={endY} r="11" fill="var(--text-primary)" />
        </g>
      ))}
      <circle cx="0" cy="0" r="22" fill="var(--amber)" />
    </svg>
  );
}

/* ── Section frame ───────────────────────────────────────────────────────────── */
function Section({ label, children, flex, minH, scrollable }) {
  return (
    <div style={{
      display: 'flex', flexDirection: 'column', gap: 8,
      flex: flex ? 1 : undefined, minHeight: minH,
      overflowY: scrollable ? 'auto' : undefined,
    }}>
      <SLabel>{label}</SLabel>
      {children}
    </div>
  );
}

/* ── TODAY card ──────────────────────────────────────────────────────────────── */
function TodayCard({ brief, sustainId, onTaskComplete }) {
  if (!brief) {
    return (
      <div style={cardStyle}>
        <Empty text="nothing scheduled · sustain state nominal" />
      </div>
    );
  }
  const { tasks_due_today = [], events_today = [], passed_strategies = [], liquid_balance = 0 } = brief;
  const noData = !tasks_due_today.length && !events_today.length && !passed_strategies.length;

  return (
    <div style={{ ...cardStyle, display: 'flex', flexDirection: 'column', gap: 10 }}>
      {/* Liquid balance */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>LIQUID</span>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 16, fontWeight: 500, color: 'var(--amber)' }}>
          KES {liquid_balance.toLocaleString()}
        </span>
      </div>

      {noData && <Empty text="nothing scheduled · sustain state nominal" />}

      {/* Tasks due today */}
      {tasks_due_today.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
          <SLabel>TASKS DUE TODAY</SLabel>
          {tasks_due_today.map(t => (
            <div key={t.id} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <button
                onClick={() => onTaskComplete(t.id)}
                style={{
                  width: 14, height: 14, borderRadius: 3, flexShrink: 0,
                  border: '1px solid var(--border-mid)', background: 'transparent',
                  cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center',
                }}
                title="Mark complete"
              />
              <span style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-primary)', flex: 1 }}>
                {t.title}
              </span>
              <span style={{
                fontFamily: 'var(--mono)', fontSize: 8,
                color: t.priority === 'high' ? 'var(--danger)' : t.priority === 'low' ? 'var(--text-dim)' : 'var(--text-muted)',
                textTransform: 'uppercase',
              }}>{t.priority}</span>
            </div>
          ))}
        </div>
      )}

      {/* Calendar events today */}
      {events_today.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
          <SLabel>CALENDAR TODAY</SLabel>
          {events_today.map(ev => (
            <div key={ev.id} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <Dot color="var(--teal)" />
              <span style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-secondary)' }}>
                {ev.title}
              </span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', marginLeft: 'auto' }}>
                {ev.date?.slice(11, 16) || ''}
              </span>
            </div>
          ))}
        </div>
      )}

      {/* Passed strategies */}
      {passed_strategies.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
          <SLabel>PASSED STRATEGIES</SLabel>
          {passed_strategies.slice(0, 3).map(p => (
            <div key={p.id} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <Dot color="var(--ok)" />
              <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)' }}>
                {p.operator_name}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/* ── Open Edits ──────────────────────────────────────────────────────────────── */
function OpenEditsCard({ proposals }) {
  const edits = proposals.filter(p => ['IN_VOTING', 'SANDBOXED', 'PASSED'].includes(p.status));
  if (!edits.length) {
    return <div style={cardStyle}><Empty text="no edits pending review" /></div>;
  }
  const badgeColor = { IN_VOTING: 'var(--amber)', PASSED: 'var(--ok)', SANDBOXED: 'var(--info)' };
  return (
    <div style={{ ...cardStyle, display: 'flex', flexDirection: 'column', gap: 8 }}>
      {edits.map(p => (
        <div key={p.id} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {p.operator_name || p.id?.slice(0, 12) + '…'}
          </span>
          <span style={{
            fontFamily: 'var(--mono)', fontSize: 7,
            color: badgeColor[p.status] || 'var(--text-muted)',
            background: `${badgeColor[p.status] || 'var(--text-muted)'}18`,
            border: `1px solid ${badgeColor[p.status] || 'var(--text-muted)'}40`,
            borderRadius: 3, padding: '1px 5px', flexShrink: 0,
          }}>{p.status}</span>
        </div>
      ))}
    </div>
  );
}

/* ── Pocket ring summary ─────────────────────────────────────────────────────── */
function PocketRingCard({ widget }) {
  if (!widget?.data?.pockets?.length) {
    return <div style={cardStyle}><Empty text="no constraints breached · all within bounds" /></div>;
  }
  const { liquid = 0, total_allocated = 0, total_spent = 0, pockets = [] } = widget.data;
  return (
    <div style={{ ...cardStyle, display: 'flex', flexDirection: 'column', gap: 6 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between' }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-muted)' }}>
          KES {liquid.toLocaleString()} liquid
        </span>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>
          {pockets.length} pocket{pockets.length !== 1 ? 's' : ''}
        </span>
      </div>
      {pockets.slice(0, 4).map(p => {
        const pct = p.pct_spent || 0;
        const barColor = pct >= 100 ? 'var(--danger)' : pct >= 80 ? 'var(--amber)' : 'var(--teal)';
        return (
          <div key={p.name} style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between' }}>
              <span style={{ fontFamily: 'var(--ui)', fontSize: 10, color: 'var(--text-secondary)' }}>{p.name}</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: barColor }}>{pct}%</span>
            </div>
            <div style={{ height: 2, background: 'var(--bg-overlay)', borderRadius: 1, overflow: 'hidden' }}>
              <div style={{ width: `${Math.min(100, pct)}%`, height: '100%', background: barColor, borderRadius: 1 }} />
            </div>
          </div>
        );
      })}
      {widget.summary && (
        <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', marginTop: 2 }}>
          {widget.summary}
        </span>
      )}
    </div>
  );
}

/* ── Constraint health summary ───────────────────────────────────────────────── */
function ConstraintHealthCard({ widget }) {
  if (!widget?.data?.constraints?.length) {
    return <div style={cardStyle}><Empty text="no constraints breached · all within bounds" /></div>;
  }
  const { constraints = [], passing = 0, total = 0 } = widget.data;
  return (
    <div style={{ ...cardStyle, display: 'flex', flexDirection: 'column', gap: 5 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 2 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: passing === total ? 'var(--ok)' : 'var(--amber)' }}>
          {passing}/{total} passing
        </span>
      </div>
      {constraints.slice(0, 5).map((c, i) => (
        <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: c.passing ? 'var(--ok)' : 'var(--danger)', flexShrink: 0 }}>
            {c.passing ? '✓' : '✗'}
          </span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {c.constraint}
          </span>
        </div>
      ))}
    </div>
  );
}

/* ── Event feed summary ──────────────────────────────────────────────────────── */
function EventFeedCard({ widget }) {
  const events = widget?.data?.events || [];
  if (!events.length) {
    return <div style={cardStyle}><Empty text="no events recorded yet" /></div>;
  }
  return (
    <div style={{ ...cardStyle, display: 'flex', flexDirection: 'column', gap: 5 }}>
      {events.slice(0, 5).map((ev, i) => (
        <div key={i} style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
          <Dot color="var(--text-dim)" />
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-secondary)', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {ev.event_name}
          </span>
          {ev.timestamp && (
            <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', flexShrink: 0 }}>
              {ev.timestamp.slice(11, 16)}
            </span>
          )}
        </div>
      ))}
    </div>
  );
}

/* ── Delegation / Council SVG ────────────────────────────────────────────────── */
function DelegationCouncil({ proposals, tick }) {
  const hasActive = proposals.some(p => p.status === 'IN_VOTING');
  const state = tick % 4;
  const stateLabel = ['DELEGATING', 'RECEIVING', 'ANSWERING', 'VOTING'][state];

  const nodes = [
    { label: 'ORCHIE',    x: 90, y: 75,  isCenter: true },
    { label: 'MENTOR',    x: 30, y: 38  },
    { label: 'CURATOR',   x: 152, y: 38 },
    { label: 'NAVIGATOR', x: 152, y: 118 },
    { label: 'PROTÉGÉ',   x: 30, y: 118 },
  ];

  if (!hasActive) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        <Empty text="council is quiet · no proposals in motion" />
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
        <Dot color="var(--amber)" pulse />
        <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--amber)', letterSpacing: '0.08em' }}>{stateLabel}</span>
      </div>
      <svg width="100%" height="150" viewBox="0 0 182 150" style={{ overflow: 'visible' }}>
        {nodes.slice(1).map((n, i) => (
          <line key={i}
            x1={nodes[0].x} y1={nodes[0].y}
            x2={n.x} y2={n.y}
            stroke="var(--border-mid)" strokeWidth="0.7" opacity="0.5"
          />
        ))}
        {nodes.map((n, i) => (
          <g key={i}>
            {n.isCenter ? (
              <circle cx={n.x} cy={n.y} r="13"
                fill="var(--amber-glow)" stroke="var(--amber-border)" strokeWidth="1"
              />
            ) : (
              <circle cx={n.x} cy={n.y} r="7"
                fill="var(--bg-raised)" stroke="var(--border-mid)" strokeWidth="1"
              />
            )}
            <text x={n.x} y={n.y + (n.isCenter ? 23 : 17)}
              textAnchor="middle" fontFamily="DM Mono" fontSize="7" fill="var(--text-muted)"
            >{n.label}</text>
          </g>
        ))}
        <g transform={`translate(${nodes[0].x - 7}, ${nodes[0].y - 7})`}>
          <SustenaMark size={14} />
        </g>
      </svg>
    </div>
  );
}

/* ── Deliberation vote bars ──────────────────────────────────────────────────── */
function DeliberationCard({ proposals }) {
  const active = proposals.find(p => p.status === 'IN_VOTING');
  if (!active) {
    return <Empty text="no vote in progress" />;
  }

  const OPERATIVES = ['MENTOR', 'CURATOR', 'NAVIGATOR', 'PROTÉGÉ'];
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
        {active.operator_name || active.id?.slice(0, 16)}
      </span>
      {OPERATIVES.map(name => (
        <div key={name} style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', width: 10, flexShrink: 0 }}>○</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)', minWidth: 54, flexShrink: 0 }}>{name}</span>
          <div style={{ flex: 1, height: 2, background: 'var(--bg-overlay)', borderRadius: 1 }}>
            <div style={{ width: '0%', height: '100%', background: 'var(--border-mid)', borderRadius: 1 }} />
          </div>
        </div>
      ))}
      <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)' }}>awaiting operative votes</span>
    </div>
  );
}

/* ── Current tasks checklist ─────────────────────────────────────────────────── */
function CurrentTasksList({ tasks, sustainId, onComplete }) {
  const today = new Date().toISOString().slice(0, 10);
  const due = tasks.filter(t => t.status !== 'completed' && t.due_date === today);

  if (!due.length) {
    return <Empty text="no active tasks · sustain is clear" />;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
      {due.slice(0, 6).map(t => (
        <div key={t.id} style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <button
            onClick={() => onComplete(t.id)}
            style={{
              width: 13, height: 13, borderRadius: 2, flexShrink: 0,
              border: '1px solid var(--border-mid)', background: 'transparent', cursor: 'pointer',
            }}
            title="Complete"
          />
          <span style={{ fontFamily: 'var(--ui)', fontSize: 11, color: 'var(--text-secondary)', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {t.title}
          </span>
        </div>
      ))}
    </div>
  );
}

/* ── Chat bubble ─────────────────────────────────────────────────────────────── */
function Bubble({ m }) {
  const isUser = m.role === 'user';
  return (
    <div className="fade-up" style={{
      display: 'flex', justifyContent: isUser ? 'flex-end' : 'flex-start', marginBottom: 6,
    }}>
      <div style={{
        maxWidth: '86%',
        padding: '7px 10px',
        borderRadius: isUser ? '10px 10px 2px 10px' : '10px 10px 10px 2px',
        background: isUser ? 'var(--amber-glow)' : 'var(--bg-raised)',
        border: `1px solid ${isUser ? 'var(--amber-border)' : 'var(--border)'}`,
        fontFamily: 'var(--ui)', fontSize: 12,
        color: 'var(--text-primary)', lineHeight: 1.5,
      }}>
        {m.text}
      </div>
    </div>
  );
}

/* ── Shared card style ───────────────────────────────────────────────────────── */
const cardStyle = {
  background: 'var(--bg-raised)', border: '1px solid var(--border)',
  borderRadius: 'var(--radius-md)', padding: '10px 12px',
};

/* ── Divider ─────────────────────────────────────────────────────────────────── */
function Divider() {
  return <div style={{ height: 1, background: 'var(--border)', flexShrink: 0 }} />;
}

/* ══════════════════════════════════════════════════════════════════════════════
   ORCHIE PANEL — main component
═══════════════════════════════════════════════════════════════════════════════ */
export default function OrchePanel() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const sustainId = searchParams.get('sustain') || 'homestead.bonnie';

  const [tick, setTick] = useState(0);
  const [brief, setBrief] = useState(null);
  const [monitorWidgets, setMonitorWidgets] = useState(null);
  const [proposals, setProposals] = useState([]);
  const [tasks, setTasks] = useState([]);
  const [messages, setMessages] = useState([]);
  const [thinking, setThinking] = useState(false);
  const [input, setInput] = useState('');
  const [listening, setListening] = useState(false);
  const scrollRef = useRef(null);

  // Tick
  useEffect(() => {
    const id = setInterval(() => setTick(t => t + 1), 2000);
    return () => clearInterval(id);
  }, []);

  // Auto-scroll chat
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: 99999, behavior: 'smooth' });
  }, [messages.length, thinking]);

  // Fetch everything on mount
  useEffect(() => {
    fetchBrief();
    fetchMonitor();
    fetchState();
  }, [sustainId]);

  async function fetchBrief() {
    try {
      const resp = await api.post('/devui/console/execute', {
        sustain_id: sustainId,
        operator: 'orchie.morning_brief',
        params: {},
      });
      const result = resp?.data?.result?.data;
      if (result) {
        setBrief(result.widget?.data || null);
        if (result.brief_text) {
          setMessages([{ role: 'assistant', text: result.brief_text }]);
        }
      }
    } catch {
      setBrief(null);
    }
  }

  async function fetchMonitor() {
    try {
      const resp = await api.get(`/devui/monitor-widgets?sustain_id=${encodeURIComponent(sustainId)}`);
      setMonitorWidgets(resp?.data?.widgets || null);
    } catch {
      setMonitorWidgets(null);
    }
  }

  async function fetchState() {
    try {
      const resp = await api.get(`/devui/state?sustain_id=${encodeURIComponent(sustainId)}`);
      const state = resp?.data?.state || {};
      setProposals(state.council_proposals || []);
      setTasks(state.tasks?.items || []);
    } catch {
      setProposals([]);
      setTasks([]);
    }
  }

  async function completeTask(taskId) {
    try {
      await api.post('/devui/console/execute', {
        sustain_id: sustainId,
        operator: 'homestead.tasks.complete',
        params: { task_id: taskId },
      });
      // Remove from local state optimistically
      setTasks(ts => ts.map(t => t.id === taskId ? { ...t, status: 'completed' } : t));
    } catch {
      window.flash?.('could not complete task · api error', 'danger');
    }
  }

  async function sendMessage() {
    const text = input.trim();
    if (!text) return;
    setInput('');
    setMessages(ms => [...ms, { role: 'user', text }]);
    setThinking(true);
    try {
      const data = await api.post('/orchie/message', { sustain_id: sustainId, message: text });
      setMessages(ms => [...ms, { role: 'assistant', text: data.reply || 'no response' }]);
    } catch {
      setMessages(ms => [...ms, { role: 'assistant', text: 'orchie is offline — start the api server and try again.' }]);
    } finally {
      setThinking(false);
    }
  }

  const recentReplies = messages.filter(m => m.role === 'assistant').slice(-3);
  const stateLabel = ['DELEGATING', 'RECEIVING', 'ANSWERING', 'VOTING'][tick % 4];

  return (
    <div style={{
      position: 'fixed', inset: 0, zIndex: 900,
      background: 'var(--bg-base)',
      display: 'flex', flexDirection: 'column',
      fontFamily: 'var(--ui)',
    }}>
      {/* ── TOP BAR ────────────────────────────────────────────────────────── */}
      <div style={{
        padding: '10px 20px', flexShrink: 0,
        borderBottom: '1px solid var(--border)',
        background: 'var(--bg-surface)',
        display: 'flex', alignItems: 'center', gap: 12,
      }}>
        <button
          onClick={() => navigate(-1)}
          style={{
            display: 'flex', alignItems: 'center', gap: 5,
            color: 'var(--text-secondary)', padding: '3px 8px',
            border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)',
            background: 'transparent', fontFamily: 'var(--mono)', fontSize: 9,
            letterSpacing: '0.07em', cursor: 'pointer', transition: 'all var(--t-fast)',
          }}
          onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
          onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
        >
          <svg width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"><path d="M6 1.5L2.5 4.5L6 7.5" /></svg>
          BACK
        </button>

        <div style={{ width: 1, height: 16, background: 'var(--border)', flexShrink: 0 }} />

        <div style={{ width: 24, height: 24, borderRadius: '50%', background: 'var(--bg-base)', border: '1px solid var(--amber-border)', display: 'flex', alignItems: 'center', justifyContent: 'center', boxShadow: '0 0 8px var(--amber-glow)', flexShrink: 0 }}>
          <SustenaMark size={14} />
        </div>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 600, letterSpacing: '0.08em', color: 'var(--text-primary)' }}>ORCHIE</span>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', letterSpacing: '0.06em' }}>{sustainId}</span>

        <div style={{ flex: 1 }} />

        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', letterSpacing: '0.07em', textTransform: 'uppercase' }}>{stateLabel}</span>
        <div style={{ display: 'flex', alignItems: 'center', gap: 5, padding: '3px 8px', background: '#2ab8a015', border: '1px solid #2ab8a040', borderRadius: 'var(--radius-sm)' }}>
          <Dot color="var(--teal)" pulse />
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--teal)', letterSpacing: '0.08em' }}>ACTIVE</span>
        </div>
      </div>

      {/* ── BODY ───────────────────────────────────────────────────────────── */}
      <div style={{
        flex: 1, minHeight: 0,
        display: 'grid',
        gridTemplateColumns: '28% 42% 30%',
        overflow: 'hidden',
      }}>

        {/* ── LEFT COLUMN ────────────────────────────────────────────────── */}
        <div style={{
          borderRight: '1px solid var(--border)',
          overflowY: 'auto', padding: '18px 16px',
          display: 'flex', flexDirection: 'column', gap: 20,
        }}>
          <Section label="TODAY">
            <TodayCard brief={brief} sustainId={sustainId} onTaskComplete={completeTask} />
          </Section>

          <Divider />

          <Section label="OPEN EDITS" flex scrollable>
            <OpenEditsCard proposals={proposals} />
          </Section>
        </div>

        {/* ── MIDDLE COLUMN ──────────────────────────────────────────────── */}
        <div style={{
          borderRight: '1px solid var(--border)',
          overflowY: 'auto', padding: '18px 16px',
          display: 'flex', flexDirection: 'column', gap: 20,
        }}>
          <Section label="PINNED MONITORING">
            <PocketRingCard widget={monitorWidgets?.pocket_ring} />
            <ConstraintHealthCard widget={monitorWidgets?.constraint_health} />
            <EventFeedCard widget={monitorWidgets?.event_feed} />
          </Section>

          <Divider />

          <Section label="RECENT ORCHIE REPLIES">
            {!recentReplies.length
              ? <Empty text="orchie hasn't spoken yet" />
              : recentReplies.map((m, i) => (
                <div key={i} style={{ ...cardStyle, borderLeft: '2px solid var(--teal)' }}>
                  <span style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.5 }}>
                    {m.text}
                  </span>
                </div>
              ))
            }
          </Section>
        </div>

        {/* ── RIGHT SIDEBAR ──────────────────────────────────────────────── */}
        <div style={{
          display: 'flex', flexDirection: 'column', overflow: 'hidden',
        }}>
          {/* Scrollable top sections */}
          <div style={{ flex: 1, minHeight: 0, overflowY: 'auto', padding: '18px 14px', display: 'flex', flexDirection: 'column', gap: 16 }}>
            <Section label="TASK / SUBTASK">
              <Empty text="no operatives reporting · all thresholds nominal" />
            </Section>

            <Divider />

            <Section label="DELEGATION / COUNCIL">
              <DelegationCouncil proposals={proposals} tick={tick} />
            </Section>

            <Divider />

            <Section label="CURRENT TASKS">
              <CurrentTasksList tasks={tasks} sustainId={sustainId} onComplete={completeTask} />
            </Section>

            <Divider />

            <Section label="DELIBERATION">
              <DeliberationCard proposals={proposals} />
            </Section>
          </div>

          {/* Pinned chat input */}
          <div style={{
            borderTop: '1px solid var(--border)', padding: '10px 14px',
            display: 'flex', flexDirection: 'column', gap: 8, flexShrink: 0,
            background: 'var(--bg-surface)',
          }}>
            {/* Chat history (compact) */}
            {messages.length > 0 && (
              <div ref={scrollRef} className="no-scrollbar" style={{
                maxHeight: 160, overflowY: 'auto',
                display: 'flex', flexDirection: 'column',
                paddingBottom: 4,
              }}>
                {messages.map((m, i) => <Bubble key={i} m={m} />)}
                {thinking && (
                  <div className="fade-up" style={{ display: 'flex', gap: 4, padding: '6px 8px' }}>
                    {[0, 1, 2].map(i => (
                      <span key={i} className="pulse" style={{ width: 4, height: 4, borderRadius: '50%', background: 'var(--text-muted)', animationDelay: `${i * 0.18}s` }} />
                    ))}
                  </div>
                )}
              </div>
            )}

            {/* LISTEN toggle */}
            <button
              onClick={() => setListening(l => !l)}
              style={{
                display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 6,
                padding: '6px', fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.1em',
                color: listening ? 'var(--teal)' : 'var(--text-muted)',
                background: listening ? 'var(--teal-glow)' : 'var(--bg-raised)',
                border: `1px solid ${listening ? 'var(--teal-border)' : 'var(--border)'}`,
                borderRadius: 'var(--radius-sm)', cursor: 'pointer', transition: 'all var(--t-fast)',
              }}
            >
              <Dot color={listening ? 'var(--teal)' : 'var(--text-dim)'} pulse={listening} />
              {listening ? 'ORCHIE IS LISTENING' : 'LISTEN'}
            </button>

            {/* Input */}
            <div style={{ display: 'flex', gap: 6 }}>
              <input
                value={input}
                onChange={e => setInput(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') sendMessage(); }}
                placeholder="Ask Orchie…"
                style={{
                  flex: 1, padding: '7px 10px',
                  fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-primary)',
                  background: 'var(--bg-base)', border: '1px solid var(--border-mid)',
                  borderRadius: 'var(--radius-sm)', outline: 'none',
                  transition: 'border-color var(--t-fast)',
                }}
                onFocus={e => e.target.style.borderColor = 'var(--amber-border)'}
                onBlur={e => e.target.style.borderColor = 'var(--border-mid)'}
              />
              <button
                onClick={sendMessage}
                style={{
                  padding: '7px 12px', flexShrink: 0,
                  background: input.trim() ? 'var(--amber)' : 'var(--bg-overlay)',
                  border: `1px solid ${input.trim() ? 'var(--amber)' : 'var(--border)'}`,
                  borderRadius: 'var(--radius-sm)',
                  color: input.trim() ? 'var(--bg-base)' : 'var(--text-dim)',
                  fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.07em',
                  cursor: 'pointer', transition: 'all var(--t-fast)',
                }}
              >SEND</button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
