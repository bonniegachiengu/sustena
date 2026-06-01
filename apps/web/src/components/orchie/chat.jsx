import React from 'react';
/* Orchie — shared chat + widget renderer. Used by sidebar (compact) and mobile (full). */

const { useState: oUseState, useEffect: oUseEffect, useRef: oUseRef, useMemo: oUseMemo } = React;

/* ─── Typewriter hook ──────────────────────────────────────
   Progressively reveals text characters at given speed (chars/tick).
*/
function useTypewriter(text, speed = 30, onDone) {
  const [shown, setShown] = oUseState('');
  const doneRef = oUseRef(false);
  oUseEffect(() => {
    setShown('');
    doneRef.current = false;
    if (!text) return;
    let i = 0;
    const interval = setInterval(() => {
      i += Math.max(1, Math.floor(speed / 12));
      if (i >= text.length) {
        setShown(text);
        clearInterval(interval);
        if (!doneRef.current) { doneRef.current = true; onDone?.(); }
      } else {
        setShown(text.slice(0, i));
      }
    }, 16);
    return () => clearInterval(interval);
  }, [text, speed]);
  return shown;
}

/* ─── Scripted conversation ────────────────────────────────
   Scripted intro messages — cleared; Orchie opens with a blank slate
   and responds via POST /orchie/message.
*/
const ORCHIE_DEMO = [];

/* ─── Widget renderers ─────────────────────────────────────
   Compact-by-default; each takes a `size` hint ('sm' for sidebar, 'md' for mobile).
*/

function MetricWidget({ data, size }) {
  return (
    <div style={{
      background: 'var(--bg-surface)', border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)', padding: size === 'sm' ? '10px 12px' : '14px 16px',
      display: 'flex', flexDirection: 'column', gap: 6,
    }}>
      <span className="label-10">{data.label}</span>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 6 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: size === 'sm' ? 26 : 36, fontWeight: 500, letterSpacing: '-0.01em', color: 'var(--text-primary)' }}>
          {data.value}
        </span>
        {data.unit && <span className="meta-10">{data.unit}</span>}
      </div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
        {data.delta && (
          <div style={{ display: 'flex', alignItems: 'center', gap: 3, color: data.delta.positive ? 'var(--teal)' : 'var(--danger)' }}>
            <span style={{ fontSize: 10 }}>{data.delta.positive ? '▲' : '▼'}</span>
            <span className="val-12" style={{ color: 'inherit', fontSize: 11 }}>{data.delta.value}</span>
            <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{data.delta.label}</span>
          </div>
        )}
        {data.sub && <span className="meta-10" style={{ color: 'var(--text-muted)', fontSize: 9 }}>{data.sub}</span>}
      </div>
    </div>
  );
}

function PieWidget({ data, size }) {
  const total = data.slices.reduce((s, x) => s + x.value, 0);
  const dim = size === 'sm' ? 88 : 120;
  const r = dim / 2 - 6;
  const cx = dim / 2, cy = dim / 2;
  const inner = r * 0.55;

  // Compute arcs
  let acc = 0;
  const arcs = data.slices.map(s => {
    const start = (acc / total) * 2 * Math.PI;
    acc += s.value;
    const end = (acc / total) * 2 * Math.PI;
    return { ...s, start, end };
  });

  const pathArc = (a) => {
    const x1 = cx + r * Math.cos(a.start - Math.PI / 2);
    const y1 = cy + r * Math.sin(a.start - Math.PI / 2);
    const x2 = cx + r * Math.cos(a.end - Math.PI / 2);
    const y2 = cy + r * Math.sin(a.end - Math.PI / 2);
    const xi2 = cx + inner * Math.cos(a.end - Math.PI / 2);
    const yi2 = cy + inner * Math.sin(a.end - Math.PI / 2);
    const xi1 = cx + inner * Math.cos(a.start - Math.PI / 2);
    const yi1 = cy + inner * Math.sin(a.start - Math.PI / 2);
    const large = a.end - a.start > Math.PI ? 1 : 0;
    return `M ${x1} ${y1} A ${r} ${r} 0 ${large} 1 ${x2} ${y2} L ${xi2} ${yi2} A ${inner} ${inner} 0 ${large} 0 ${xi1} ${yi1} Z`;
  };

  return (
    <div style={{
      background: 'var(--bg-surface)', border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)', padding: size === 'sm' ? '10px 12px' : '14px 16px',
      display: 'flex', gap: 14, alignItems: 'center',
    }}>
      <div style={{ position: 'relative', flexShrink: 0 }}>
        <svg width={dim} height={dim}>
          {arcs.map((a, i) => (
            <path
              key={i}
              d={pathArc(a)}
              fill={a.color}
              style={{
                opacity: 0,
                animation: `fadeUp 0.4s ease-out ${0.15 + i * 0.08}s forwards`,
                transformOrigin: `${cx}px ${cy}px`,
              }}
            />
          ))}
        </svg>
        <div style={{
          position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column',
          alignItems: 'center', justifyContent: 'center', pointerEvents: 'none',
        }}>
          <span className="label-10" style={{ fontSize: 8 }}>TOTAL</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: size === 'sm' ? 10 : 12, fontWeight: 500, color: 'var(--text-primary)' }}>{data.total}</span>
        </div>
      </div>
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 4, minWidth: 0 }}>
        {data.slices.map((s, i) => (
          <div key={i} className="fade-up" style={{
            display: 'grid', gridTemplateColumns: '8px 1fr auto', gap: 8, alignItems: 'center',
            animationDelay: `${0.25 + i * 0.06}s`,
          }}>
            <span style={{ width: 7, height: 7, background: s.color, borderRadius: 1 }} />
            <span style={{ fontFamily: 'var(--ui)', fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{s.label}</span>
            <span className="val-12" style={{ fontSize: 10, color: 'var(--text-secondary)' }}>{s.value}%</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function LineWidget({ data, size }) {
  const W = size === 'sm' ? 280 : 360;
  const H = size === 'sm' ? 80 : 100;
  const series = data.series;
  const min = Math.min(...series, data.threshold || Infinity) * 0.9;
  const max = Math.max(...series, data.threshold || -Infinity) * 1.05;
  const range = max - min || 1;
  const pts = series.map((v, i) => {
    const x = (i / (series.length - 1)) * (W - 24) + 12;
    const y = H - ((v - min) / range) * H + 8;
    return [x, y];
  });
  const linePath = pts.map(([x, y], i) => `${i === 0 ? 'M' : 'L'} ${x} ${y}`).join(' ');
  const lastY = pts[pts.length - 1][1];
  const lastX = pts[pts.length - 1][0];
  const thrY = data.threshold ? (H - ((data.threshold - min) / range) * H + 8) : null;

  return (
    <div style={{
      background: 'var(--bg-surface)', border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)', padding: size === 'sm' ? '10px 12px' : '14px 16px',
      display: 'flex', flexDirection: 'column', gap: 6,
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
        <span className="label-10">BURN RATE · LAST 12 DAYS</span>
        <span className="val-12" style={{ fontSize: 11, color: 'var(--amber)' }}>{series[series.length - 1].toLocaleString()} {data.unit}</span>
      </div>
      <svg width={W} height={H + 18} style={{ overflow: 'visible' }}>
        {/* Threshold line */}
        {thrY != null && (
          <>
            <line x1="0" y1={thrY} x2={W} y2={thrY} stroke="var(--danger)" strokeDasharray="2 3" strokeWidth="1" opacity="0.6" />
            <text x={W - 4} y={thrY - 4} textAnchor="end" fontFamily="DM Mono" fontSize="9" fill="var(--danger)">CEILING {data.threshold}</text>
          </>
        )}
        {/* Area */}
        <path
          d={`${linePath} L ${lastX} ${H + 8} L 12 ${H + 8} Z`}
          fill="var(--amber-glow)"
          style={{ opacity: 0, animation: 'fadeUp 0.5s ease-out 0.4s forwards' }}
        />
        {/* Line */}
        <path d={linePath} fill="none" stroke="var(--amber)" strokeWidth="1.4"
          className="draw-line"
          style={{ '--dash-len': W * 3, animationDuration: '1s' }}
        />
        {/* End dot */}
        <circle cx={lastX} cy={lastY} r="3" fill="var(--amber)"
          style={{ opacity: 0, animation: 'fadeUp 0.3s ease-out 1.1s forwards' }}
        />
        {/* Labels */}
        {data.labels?.map((l, i) => {
          if (!l) return null;
          const x = (i / (series.length - 1)) * (W - 24) + 12;
          return <text key={i} x={x} y={H + 18} textAnchor="middle" fontFamily="DM Mono" fontSize="9" fill="var(--text-muted)">{l}</text>;
        })}
      </svg>
    </div>
  );
}

function MpesaWidget({ data, size }) {
  return (
    <div style={{
      background: 'var(--bg-surface)',
      border: '1px solid var(--amber-border)',
      borderRadius: 'var(--radius-md)',
      padding: size === 'sm' ? '12px 14px' : '14px 16px',
      display: 'flex', flexDirection: 'column', gap: 10,
      position: 'relative', overflow: 'hidden',
    }}>
      <span style={{ position: 'absolute', top: 0, left: 0, right: 0, height: 2, background: 'var(--amber)' }} />
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
        <span className="label-10" style={{ color: 'var(--amber)' }}>M-PESA · STK PUSH</span>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>TILL · {data.till}</span>
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
        <span style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-secondary)' }}>To</span>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-primary)' }}>{data.recipient}</span>
      </div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 4 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 24, fontWeight: 500, color: 'var(--text-primary)' }}>{data.amount}</span>
      </div>
      <span className="meta-10" style={{ color: 'var(--text-muted)' }}>memo · {data.memo}</span>
      <div style={{ display: 'flex', gap: 6, marginTop: 4 }}>
        <button onClick={() => window.openPinPad?.({
          title: `M-Pesa STK · ${data.amount}`,
          body: `To ${data.recipient}. ${data.memo}.`,
          onConfirm: () => window.flash?.(`Sent ${data.amount} to ${data.recipient}`, 'ok'),
        })} style={{
          flex: 1, padding: '8px',
          fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
          background: 'var(--amber)', color: 'var(--bg-base)',
          border: '1px solid var(--amber)', borderRadius: 'var(--radius-sm)',
        }}>APPROVE · PIN</button>
        <button onClick={() => window.flash?.('Deferred · re-prompt in 30 min', 'info')} style={{
          padding: '8px 12px',
          fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
          background: 'transparent', color: 'var(--text-secondary)',
          border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
        }}>DEFER</button>
      </div>
    </div>
  );
}

function AlertWidget({ data, size }) {
  const c = data.tone === 'amber' ? 'var(--amber)' : data.tone === 'danger' ? 'var(--danger)' : 'var(--teal)';
  return (
    <div style={{
      background: 'var(--bg-surface)',
      border: `1px solid ${c}`,
      borderRadius: 'var(--radius-md)',
      padding: '12px 14px',
      display: 'flex', flexDirection: 'column', gap: 6,
      borderLeftWidth: 3,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <Icon name="warning" size={12} />
        <span className="label-10" style={{ color: c }}>{data.title.toUpperCase()}</span>
      </div>
      <span style={{ fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.5 }}>{data.body}</span>
      {data.actions && (
        <div style={{ display: 'flex', gap: 6, marginTop: 4 }}>
          {data.actions.map((a, i) => (
            <button key={a} onClick={() => window.flash?.(`${a} · ${data.title.toLowerCase()}`, i === 0 ? 'ok' : 'info')} style={{
              padding: '5px 10px',
              fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
              color: i === 0 ? c : 'var(--text-muted)',
              background: i === 0 ? 'transparent' : 'transparent',
              border: `1px solid ${i === 0 ? c : 'var(--border-mid)'}`,
              borderRadius: 'var(--radius-sm)',
            }}>{a.toUpperCase()}</button>
          ))}
        </div>
      )}
    </div>
  );
}

function ProposalWidget({ data, size }) {
  const total = data.for + data.against + data.abstain;
  const forPct = (data.for / total) * 100;
  const againstPct = (data.against / total) * 100;
  const abstainPct = (data.abstain / total) * 100;
  return (
    <div style={{
      background: 'var(--bg-surface)',
      border: '1px solid var(--amber-border)',
      borderRadius: 'var(--radius-md)',
      padding: '12px 14px',
      display: 'flex', flexDirection: 'column', gap: 10,
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 8 }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
          <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{data.id} · COUNCIL</span>
          <span style={{ fontFamily: 'var(--ui)', fontSize: 12, fontWeight: 500, color: 'var(--text-primary)', lineHeight: 1.35 }}>{data.title}</span>
        </div>
        <Badge tone="amber" dot>VOTING</Badge>
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between' }}>
          <span className="meta-10" style={{ fontSize: 9 }}>QUORUM</span>
          <span className="val-12" style={{ fontSize: 10, color: total >= data.quorum ? 'var(--teal)' : 'var(--amber)' }}>{total}/{data.quorum}</span>
        </div>
        <div style={{ display: 'flex', height: 4, borderRadius: 2, overflow: 'hidden', background: 'var(--bg-base)' }}>
          <div style={{ width: `${forPct}%`, background: 'var(--teal)' }} />
          <div style={{ width: `${againstPct}%`, background: 'var(--danger)' }} />
          <div style={{ width: `${abstainPct}%`, background: 'var(--text-muted)' }} />
        </div>
        <div style={{ display: 'flex', gap: 10 }}>
          <span className="meta-10" style={{ fontSize: 9, color: 'var(--teal)' }}>YES {data.for}</span>
          <span className="meta-10" style={{ fontSize: 9, color: 'var(--danger)' }}>NO {data.against}</span>
          <span className="meta-10" style={{ fontSize: 9, color: 'var(--text-muted)' }}>ABSTAIN {data.abstain}</span>
        </div>
      </div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', paddingTop: 8, borderTop: '1px solid var(--border)' }}>
        <span className="meta-10">CLOSES IN {data.eta}</span>
        <PBtn onClick={() => window.confirmAction?.({
          title: `Vote on ${data.id}?`,
          body: `${data.title}. Council quorum at ${data.for + data.against + data.abstain} / ${data.quorum}.`,
          ctaLabel: 'CAST · YES',
          tone: 'amber',
          onConfirm: () => window.flash?.(`Vote cast · YES on ${data.id}`, 'ok'),
        })}>VOTE</PBtn>
      </div>
    </div>
  );
}

/* Composite widget switch */
function OrchieWidget({ widget, size = 'sm' }) {
  if (!widget) return null;
  switch (widget.type) {
    case 'metric':   return <MetricWidget data={widget.data} size={size} />;
    case 'pie':      return <PieWidget data={widget.data} size={size} />;
    case 'line':     return <LineWidget data={widget.data} size={size} />;
    case 'mpesa':    return <MpesaWidget data={widget.data} size={size} />;
    case 'alert':    return <AlertWidget data={widget.data} size={size} />;
    case 'proposal': return <ProposalWidget data={widget.data} size={size} />;
    default: return null;
  }
}

/* ─── Animated Orchie message bubble ───────────────────────
   Reveals text via typewriter then fades widget in.
*/
function OrchieBubble({ m, size = 'sm', onDone }) {
  const [widgetReady, setWidgetReady] = oUseState(false);
  const text = useTypewriter(m.text || '', 30, () => setTimeout(() => { setWidgetReady(true); onDone?.(); }, 80));
  const isUser = m.role === 'user';
  const compact = size === 'sm';

  if (isUser) {
    return (
      <div className="fade-up" style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
        <div className="bubble-user" style={{
          maxWidth: compact ? '85%' : '78%',
          fontSize: compact ? 12 : 14,
          fontFamily: 'var(--ui)', color: 'var(--text-primary)', lineHeight: 1.5,
        }}>{m.text}</div>
      </div>
    );
  }

  return (
    <div className="fade-up" style={{ display: 'flex', gap: 10, alignItems: 'flex-start' }}>
      {!compact && (
        <div style={{
          width: 28, height: 28, flexShrink: 0,
          borderRadius: '50%',
          background: 'var(--bg-surface)', border: '1px solid var(--amber-border)',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          boxShadow: '0 0 12px var(--amber-glow)',
        }}>
          <SustenaMark size={18} />
        </div>
      )}
      <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 8 }}>
        <div className="bubble" style={{
          fontFamily: 'var(--ui)',
          fontSize: compact ? 12 : 14,
          color: 'var(--text-primary)',
          lineHeight: 1.55,
        }}>
          {compact && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
              <SustenaMark size={12} />
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--amber)', letterSpacing: '0.08em' }}>ORCHIE</span>
            </div>
          )}
          {text}
          {text.length < (m.text?.length || 0) && <span className="t-cursor" style={{ width: 5, height: 11 }} />}
        </div>
        {m.widget && widgetReady && (
          <div className="fade-up">
            <OrchieWidget widget={m.widget} size={size} />
          </div>
        )}
      </div>
    </div>
  );
}

/* Thinking dots */
function OrchieThinking({ size = 'sm' }) {
  return (
    <div className="fade-up" style={{ display: 'flex', gap: 10, alignItems: 'flex-start' }}>
      {size === 'md' && (
        <div style={{
          width: 28, height: 28, flexShrink: 0, borderRadius: '50%',
          background: 'var(--bg-surface)', border: '1px solid var(--amber-border)',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
        }}>
          <SustenaMark size={18} />
        </div>
      )}
      <div className="bubble" style={{ display: 'flex', gap: 4, padding: '10px 14px' }}>
        {[0, 1, 2].map(i => (
          <span key={i} className="pulse" style={{
            width: 5, height: 5, borderRadius: '50%', background: 'var(--text-muted)',
            animationDelay: `${i * 0.18}s`,
          }} />
        ))}
      </div>
    </div>
  );
}

/* Streamed conversation hook — plays scripted messages with delays, then routes
   live user messages to POST /orchie/message on the real API.
*/
function useStreamedConversation(script, autoStart = true, baseDelay = 1200, sustainId = 'homestead.bonnie') {
  const [shown, setShown] = oUseState([]);
  const [thinking, setThinking] = oUseState(false);
  const idxRef = oUseRef(0);

  const playNext = () => {
    if (idxRef.current >= script.length) return;
    setThinking(true);
    const msg = script[idxRef.current];
    const thinkTime = 400 + Math.random() * 600;
    setTimeout(() => {
      setThinking(false);
      setShown(s => [...s, msg]);
      idxRef.current += 1;
    }, thinkTime);
  };

  // Auto-start sequence
  oUseEffect(() => {
    if (!autoStart) return;
    if (shown.length === 0 && idxRef.current === 0) {
      const t0 = setTimeout(() => playNext(), 600);
      return () => clearTimeout(t0);
    }
  }, []);

  const onMessageDone = () => {
    setTimeout(() => playNext(), baseDelay);
  };

  const send = async (text) => {
    setShown(s => [...s, { role: 'user', text }]);
    setThinking(true);
    try {
      const res = await fetch('http://localhost:8000/orchie/message', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sustain_id: sustainId, message: text }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      setThinking(false);
      setShown(s => [...s, { role: 'assistant', text: data.reply }]);
    } catch (err) {
      setThinking(false);
      setShown(s => [...s, {
        role: 'assistant',
        text: 'Orchie is offline — start the API server and try again.',
        widget: null,
      }]);
    }
  };

  const restart = () => {
    setShown([]);
    idxRef.current = 0;
    setThinking(false);
    setTimeout(() => playNext(), 400);
  };

  return { messages: shown, thinking, onMessageDone, send, restart };
}

function pickReply(text) {
  const t = text.toLowerCase();
  if (t.includes('budget') || t.includes('pocket')) {
    return {
      role: 'assistant',
      text: "Here's the pocket breakdown again, with overspend flagged in amber:",
      widget: { type: 'pie', data: ORCHIE_DEMO[2].widget.data },
    };
  }
  if (t.includes('burn') || t.includes('rate')) {
    return {
      role: 'assistant',
      text: "Burn rate over the last 12 days — we're 6% under ceiling but trending up:",
      widget: { type: 'line', data: ORCHIE_DEMO[3].widget.data },
    };
  }
  if (t.includes('reall') || t.includes('move') || t.includes('reallocate')) {
    return {
      role: 'assistant',
      text: "Simulated. Moving KSH 400k from Treasury Ops → Carbon-R&D keeps you 6.1% above the council floor and unblocks two grants by 9 days. Draft as SUS-0149?",
      widget: { type: 'proposal', data: { ...ORCHIE_DEMO[6].widget.data, id: 'SUS-0149 (DRAFT)', title: 'Reallocate −KSH 400k Ops → Carbon-R&D', for: 0, against: 0, abstain: 0, eta: 'NOT QUEUED' } },
    };
  }
  if (t.includes('cash') || t.includes('position')) {
    return {
      role: 'assistant',
      text: 'Right now:',
      widget: { type: 'metric', data: ORCHIE_DEMO[1].widget.data },
    };
  }
  return {
    role: 'assistant',
    text: "Pulling that — I'll need a moment to simulate the dependency chain. Should I surface the impact on Q3 cap, or just the headline number?",
  };
}

Object.assign(window, {
  useTypewriter, ORCHIE_DEMO,
  OrchieWidget, OrchieBubble, OrchieThinking,
  useStreamedConversation, pickReply,
  MetricWidget, PieWidget, LineWidget, MpesaWidget, AlertWidget, ProposalWidget,
});
