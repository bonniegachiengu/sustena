import React from 'react';
/* Orchie mobile — full screen iOS frame, dark, chat with animated widgets */

const { useState: mUseState, useEffect: mUseEffect, useRef: mUseRef } = React;

function OrchieMobileApp() {
  const { messages, thinking, onMessageDone, send, restart } = useStreamedConversation(ORCHIE_DEMO, true, 1500);
  const [input, setInput] = mUseState('');
  const scrollerRef = mUseRef(null);

  // Scale device to fit viewport (max 0.92)
  const [scale, setScale] = mUseState(1);
  mUseEffect(() => {
    const update = () => {
      const margin = 60;
      const sh = (window.innerHeight - margin) / 874;
      const sw = (window.innerWidth - margin) / 402;
      setScale(Math.min(1, sh, sw));
    };
    update();
    window.addEventListener('resize', update);
    return () => window.removeEventListener('resize', update);
  }, []);

  mUseEffect(() => {
    scrollerRef.current?.scrollTo({ top: 999999, behavior: 'smooth' });
  }, [messages.length, thinking]);

  const submit = () => {
    if (!input.trim()) return;
    send(input.trim());
    setInput('');
  };

  return (
    <div style={{
      width: '100vw', height: '100vh', overflow: 'hidden',
      background: 'var(--bg-base)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      position: 'relative',
    }}>
      {/* Backdrop atmospherics */}
      <div style={{
        position: 'absolute', inset: 0,
        background: `
          radial-gradient(ellipse 800px 600px at 30% 20%, var(--amber-glow), transparent 60%),
          radial-gradient(ellipse 600px 500px at 70% 80%, var(--teal-glow), transparent 60%)
        `,
        opacity: 0.5, pointerEvents: 'none',
      }} />

      {/* Side info — desktop only */}
      <div style={{
        position: 'absolute', left: 40, top: 40,
        display: 'flex', flexDirection: 'column', gap: 8,
      }}>
        <SustenaLogo size={16} />
        <div className="label-10" style={{ marginTop: 16 }}>HUMAN-AGENT INTERFACE</div>
        <h1 style={{ fontFamily: 'var(--ui)', fontSize: 32, fontWeight: 500, letterSpacing: '-0.02em', color: 'var(--text-primary)', lineHeight: 1.05, maxWidth: 320 }}>
          Orchie · <span style={{ color: 'var(--amber)' }}>your delegate</span> in the Sustenaverse
        </h1>
        <p style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-secondary)', lineHeight: 1.6, maxWidth: 320, marginTop: 4 }}>
          Mobile-first. WhatsApp-compatible. Widget-driven. Every recommendation
          carries simulation evidence; the user holds the 51% vote.
        </p>
      </div>

      {/* Side meta — bottom-left */}
      <div style={{
        position: 'absolute', left: 40, bottom: 40,
        display: 'flex', flexDirection: 'column', gap: 6,
      }}>
        <span className="label-10" style={{ fontSize: 9 }}>RUNTIME</span>
        <div className="meta-11" style={{ color: 'var(--text-secondary)' }}>haiku-4.5 · sonnet-fallback</div>
        <div className="meta-11" style={{ color: 'var(--text-secondary)' }}>p95 inference · 412 ms</div>
        <div className="meta-11" style={{ color: 'var(--text-secondary)' }}>privilege · L1 delegate</div>
      </div>

      {/* Right-side controls */}
      <div style={{
        position: 'absolute', right: 40, top: 40,
        display: 'flex', flexDirection: 'column', gap: 12, alignItems: 'flex-end',
      }}>
        <button onClick={restart} style={{
          padding: '6px 12px',
          fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.08em', fontWeight: 500,
          color: 'var(--text-secondary)',
          background: 'var(--bg-surface)', border: '1px solid var(--border-mid)',
          borderRadius: 'var(--radius-sm)',
          display: 'inline-flex', alignItems: 'center', gap: 6,
          transition: 'all var(--t-fast)',
        }}
        onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
        onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border-mid)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
        ><Icon name="replay" size={11} /> REPLAY DEMO</button>
        <a href="Sustena Dashboard.html" style={{
          padding: '6px 12px',
          fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.08em', fontWeight: 500,
          color: 'var(--text-muted)',
          background: 'transparent', border: '1px solid var(--border)',
          borderRadius: 'var(--radius-sm)',
          textDecoration: 'none',
          transition: 'all var(--t-fast)',
        }}>← MYCELIUM CONTROL PANEL</a>
      </div>

      {/* The device */}
      <div style={{ position: 'relative', zIndex: 2, transform: `scale(${scale})`, transformOrigin: 'center center' }}>
        <IOSDevice width={402} height={874} dark>
          <OrchieScreen
            messages={messages}
            thinking={thinking}
            onMessageDone={onMessageDone}
            send={send}
            input={input}
            setInput={setInput}
            submit={submit}
            scrollerRef={scrollerRef}
          />
        </IOSDevice>
      </div>
    </div>
  );
}

function OrchieScreen({ messages, thinking, onMessageDone, send, input, setInput, submit, scrollerRef }) {
  const [focused, setFocused] = mUseState(false);
  return (
    <div style={{
      display: 'flex', flexDirection: 'column', height: '100%',
      background: 'var(--bg-base)',
      paddingTop: 54, /* below status bar */
    }}>
      {/* Persona header */}
      <header style={{
        padding: '8px 16px 12px',
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        borderBottom: '1px solid var(--border)',
        flexShrink: 0,
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <div style={{
            position: 'relative',
            width: 40, height: 40, borderRadius: '50%',
            background: 'var(--bg-surface)', border: '1px solid var(--amber-border)',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            boxShadow: '0 0 16px var(--amber-glow)',
          }}>
            <SustenaMark size={24} />
            <span style={{
              position: 'absolute', bottom: -1, right: -1,
              width: 10, height: 10, borderRadius: '50%',
              background: 'var(--teal)', border: '2px solid var(--bg-base)',
            }} />
          </div>
          <div style={{ display: 'flex', flexDirection: 'column' }}>
            <span style={{ fontFamily: 'var(--ui)', fontSize: 15, fontWeight: 600, color: 'var(--text-primary)' }}>Orchie</span>
            <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
              <span className="pulse" style={{ width: 4, height: 4, borderRadius: '50%', background: 'var(--teal)' }} />
              <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--teal)', letterSpacing: '0.06em' }}>ONLINE · 28 ms</span>
            </div>
          </div>
        </div>
        <div style={{ display: 'flex', gap: 6 }}>
          <HeaderBtn icon={<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.4"><circle cx="7" cy="7" r="5.5"/><path d="M7 4v3.5l2 2"/></svg>} />
          <HeaderBtn icon={<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.4"><path d="M2 4h10M2 7h10M2 10h6"/></svg>} />
        </div>
      </header>

      {/* Context strip */}
      <div className="no-scrollbar" style={{
        padding: '8px 16px',
        display: 'flex', gap: 6, overflow: 'auto',
        borderBottom: '1px solid var(--border)',
        flexShrink: 0,
      }}>
        <ContextChip active>HOMESTEAD</ContextChip>
        <ContextChip>BIASHARA</ContextChip>
        <ContextChip>CHAMA</ContextChip>
        <ContextChip>+2</ContextChip>
      </div>

      {/* Messages */}
      <div ref={scrollerRef} className="no-scrollbar" style={{
        flex: 1, minHeight: 0, overflowY: 'auto', overflowX: 'hidden',
        padding: '16px 14px 16px',
        display: 'flex', flexDirection: 'column', gap: 14,
      }}>
        <DayDivider>TODAY · 14:32 EAT</DayDivider>
        {messages.map((m, i) => (
          <OrchieBubble
            key={i}
            m={m}
            size="md"
            onDone={i === messages.length - 1 ? onMessageDone : undefined}
          />
        ))}
        {thinking && <OrchieThinking size="md" />}
      </div>

      {/* Suggestions */}
      <div className="no-scrollbar" style={{
        padding: '8px 14px 6px',
        display: 'flex', gap: 6, overflow: 'auto',
        flexShrink: 0,
      }}>
        {['Show pockets', 'Why is burn rising?', 'Reallocate 400k', 'Pantry'].map(s => (
          <button key={s} onClick={() => send(s)} style={{
            padding: '6px 12px',
            fontFamily: 'var(--ui)', fontSize: 12,
            color: 'var(--text-secondary)',
            background: 'var(--bg-surface)',
            border: '1px solid var(--border)',
            borderRadius: 16,
            whiteSpace: 'nowrap',
            flexShrink: 0,
            transition: 'all var(--t-fast)',
          }}
          onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
          onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
          >{s}</button>
        ))}
      </div>

      {/* Composer */}
      <div style={{
        padding: '8px 12px 16px',
        display: 'flex', alignItems: 'flex-end', gap: 8,
        borderTop: '1px solid var(--border)',
        flexShrink: 0,
      }}>
        <button style={{
          width: 36, height: 36, borderRadius: '50%',
          background: 'var(--bg-surface)', border: '1px solid var(--border-mid)',
          color: 'var(--text-secondary)',
          display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0,
        }}><Icon name="plus" size={14} /></button>
        <div style={{
          flex: 1, position: 'relative',
          background: 'var(--bg-surface)',
          border: '1px solid ' + (focused ? 'var(--amber-border)' : 'var(--border-mid)'),
          borderRadius: 22,
          padding: '8px 14px',
          minHeight: 36,
          transition: 'border-color var(--t-fast)',
        }}>
          <input
            value={input} onChange={e => setInput(e.target.value)}
            onFocus={() => setFocused(true)} onBlur={() => setFocused(false)}
            onKeyDown={e => { if (e.key === 'Enter') submit(); }}
            placeholder="Ask Orchie..."
            style={{
              width: '100%', background: 'transparent', border: 0, outline: 'none',
              fontFamily: 'var(--ui)', fontSize: 14, color: 'var(--text-primary)',
            }}
          />
        </div>
        <button onClick={submit} disabled={!input.trim()} style={{
          width: 36, height: 36, borderRadius: '50%',
          background: input.trim() ? 'var(--amber)' : 'var(--bg-surface)',
          color: input.trim() ? 'var(--bg-base)' : 'var(--text-muted)',
          border: '1px solid ' + (input.trim() ? 'var(--amber)' : 'var(--border-mid)'),
          display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0,
          transition: 'all var(--t-fast)',
        }}><Icon name="send" size={14} /></button>
      </div>
    </div>
  );
}

function HeaderBtn({ icon }) {
  return (
    <button style={{
      width: 32, height: 32, borderRadius: '50%',
      background: 'var(--bg-surface)', border: '1px solid var(--border)',
      color: 'var(--text-secondary)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      transition: 'all var(--t-fast)',
    }}
    onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
    onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
    >{icon}</button>
  );
}

function ContextChip({ children, active }) {
  return (
    <span style={{
      padding: '4px 10px',
      fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.08em', fontWeight: 500,
      color: active ? 'var(--amber)' : 'var(--text-secondary)',
      background: active ? 'var(--amber-glow)' : 'var(--bg-surface)',
      border: '1px solid ' + (active ? 'var(--amber-border)' : 'var(--border)'),
      borderRadius: 12,
      whiteSpace: 'nowrap',
    }}>{children}</span>
  );
}

function DayDivider({ children }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '4px 0' }}>
      <div style={{ flex: 1, height: 1, background: 'var(--border)' }} />
      <span className="label-10" style={{ color: 'var(--text-muted)', fontSize: 9 }}>{children}</span>
      <div style={{ flex: 1, height: 1, background: 'var(--border)' }} />
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<OrchieMobileApp />);
