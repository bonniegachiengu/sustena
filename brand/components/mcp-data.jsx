/* Sustena — Shared data models + DAG rendering components
   DagNode (Step 6: text-first, icon secondary, min 13px labels)
   DagEdges (Step 6: reduced opacity, selected highlight)
   CodeEditor (Step 6: Python syntax highlighting, line numbers)
   OrchieExpansionPanel (Step 4: intent cards + ambient activity)
   All data: SUSTAINS, STATE_TREE, OPERATIVES, SIM_NODES/EDGES, etc.
*/

const { useState: dUseState, useEffect: dUseEffect, useMemo: dUseMemo, useRef: dUseRef, useCallback: dUseCallback } = React;

/* ═══════════════════════════════════════════════════════════
   NODE COLOUR + ICON CONFIG
   ═══════════════════════════════════════════════════════════ */
const NODE_STYLES = {
  state:      { bg: 'var(--node-state)',      border: 'var(--node-state)',      icon: '▭', label: 'STATE' },
  operator:   { bg: 'var(--node-operator)',   border: 'var(--node-operator)',   icon: '◖◗', label: 'OP'   },
  constraint: { bg: 'var(--node-constraint)', border: 'var(--node-constraint)', icon: '◆',  label: 'CSTR' },
  event:      { bg: 'var(--node-event)',      border: 'var(--node-event)',      icon: '⬡',  label: 'EVT'  },
  operative:  { bg: 'var(--node-operative)',  border: 'var(--node-operative)',  icon: '⬢',  label: 'AGT'  },
  gate:       { bg: 'var(--text-muted)',      border: 'var(--text-muted)',      icon: '⊕',  label: 'GATE' },
};

/* ═══════════════════════════════════════════════════════════
   DAG EDGES — reduced opacity (0.4), selected = 1.0 + amber
   ═══════════════════════════════════════════════════════════ */
function DagEdges({ nodes, edges, activePath, width, height }) {
  const nodeMap = Object.fromEntries(nodes.map(n => [n.id, n]));
  const NODE_W = 130, NODE_H = 46;

  return (
    <svg
      width={width} height={height}
      style={{ position: 'absolute', inset: 0, pointerEvents: 'none', overflow: 'visible' }}
    >
      <defs>
        <marker id="arrowhead" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto">
          <path d="M 0 1 L 6 3.5 L 0 6 Z" fill="var(--text-dim)" />
        </marker>
        <marker id="arrowhead-active" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto">
          <path d="M 0 1 L 6 3.5 L 0 6 Z" fill="var(--amber)" />
        </marker>
      </defs>
      {edges.map(([fromId, toId], i) => {
        const from = nodeMap[fromId];
        const to   = nodeMap[toId];
        if (!from || !to) return null;
        const x1 = from.x + NODE_W;
        const y1 = from.y + NODE_H / 2;
        const x2 = to.x;
        const y2 = to.y + NODE_H / 2;
        const dx = (x2 - x1) * 0.45;
        const path = `M ${x1} ${y1} C ${x1+dx} ${y1} ${x2-dx} ${y2} ${x2} ${y2}`;
        const isActive = activePath.includes(fromId) && activePath.includes(toId);
        return (
          <path
            key={i}
            d={path}
            fill="none"
            stroke={isActive ? 'var(--amber)' : 'var(--text-dim)'}
            strokeWidth={isActive ? 1.4 : 0.9}
            opacity={isActive ? 1.0 : 0.4}
            markerEnd={isActive ? 'url(#arrowhead-active)' : 'url(#arrowhead)'}
            strokeDasharray={isActive ? 'none' : 'none'}
          />
        );
      })}
    </svg>
  );
}

/* ═══════════════════════════════════════════════════════════
   DAG NODE — text label prominent, icon top-right corner
   Step 6: min 13px label, label-first, icon is secondary
   ═══════════════════════════════════════════════════════════ */
function DagNode({ node, active, pulse, onClick, onOrchieExpand }) {
  const [hovered, setHovered] = dUseState(false);
  const style = NODE_STYLES[node.type] || NODE_STYLES.state;
  const isActive = active || hovered;
  const isOrchie = node.type === 'operative' && node.label.toLowerCase().includes('orchie');
  const isOperator = node.type === 'operator';
  const W = 130, H = 46;

  const handleClick = () => {
    if (isOrchie && onOrchieExpand) {
      onOrchieExpand(node);
    } else if (isOperator && onClick) {
      onClick(node);
    } else if (onClick) {
      onClick(node);
    }
  };

  return (
    <div
      onClick={handleClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        position: 'absolute',
        left: node.x, top: node.y,
        width: W, height: H,
        background: isActive ? `${style.bg}22` : 'var(--bg-surface)',
        border: `1.5px solid ${isActive ? style.bg : 'var(--border-mid)'}`,
        borderRadius: 'var(--radius-md)',
        boxShadow: pulse
          ? `0 0 12px ${style.bg}60, 0 0 24px ${style.bg}25`
          : (isActive ? `0 4px 14px rgba(0,0,0,0.4)` : `0 2px 6px rgba(0,0,0,0.25)`),
        cursor: (isOrchie || isOperator) ? 'pointer' : 'default',
        transition: 'all var(--t-mid)',
        display: 'flex', alignItems: 'center',
        padding: '0 10px 0 10px',
        gap: 8,
        animation: pulse ? 'dagPulse 1.8s ease-in-out infinite' : 'none',
        userSelect: 'none',
        overflow: 'hidden',
      }}
    >
      <style>{`
        @keyframes dagPulse {
          0%, 100% { box-shadow: 0 0 8px ${style.bg}50; }
          50%       { box-shadow: 0 0 18px ${style.bg}80, 0 0 32px ${style.bg}30; }
        }
      `}</style>

      {/* Label — PROMINENT, min 13px */}
      <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 1 }}>
        {/* Node type tag (tiny, above label) */}
        <span style={{
          fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500,
          letterSpacing: '0.08em', textTransform: 'uppercase',
          color: isActive ? style.bg : 'var(--text-dim)',
          lineHeight: 1,
        }}>
          {style.label}
        </span>
        {/* Main label — min 13px, text with background if on dark canvas */}
        <span style={{
          fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 500,
          color: isActive ? 'var(--text-primary)' : 'var(--text-secondary)',
          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          lineHeight: 1.2,
          /* Subtle label backing for legibility */
          textShadow: '0 1px 3px rgba(0,0,0,0.5)',
        }}>
          {node.label}
        </span>
      </div>

      {/* Icon — secondary, top-right corner */}
      <span style={{
        fontFamily: 'var(--mono)', fontSize: 13,
        color: isActive ? style.bg : 'var(--text-dim)',
        flexShrink: 0,
        opacity: isActive ? 1 : 0.55,
        transition: 'opacity var(--t-fast)',
      }}>
        {style.icon}
      </span>

      {/* Active/pulse glow bar at bottom */}
      {isActive && (
        <span style={{
          position: 'absolute', bottom: 0, left: 0, right: 0, height: 2,
          background: style.bg, opacity: pulse ? 1 : 0.6,
          animation: pulse ? 'pulse 1.6s ease-in-out infinite' : 'none',
        }} />
      )}

      {/* Orchie expand hint */}
      {isOrchie && hovered && (
        <span style={{
          position: 'absolute', top: -18, left: '50%', transform: 'translateX(-50%)',
          fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--amber)',
          background: 'var(--bg-raised)', padding: '2px 6px', borderRadius: 4,
          border: '1px solid var(--amber-border)', whiteSpace: 'nowrap',
          pointerEvents: 'none',
        }}>
          EXPAND ↗
        </span>
      )}

      {/* Operator code hint */}
      {isOperator && hovered && (
        <span style={{
          position: 'absolute', top: -18, left: '50%', transform: 'translateX(-50%)',
          fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--node-operator)',
          background: 'var(--bg-raised)', padding: '2px 6px', borderRadius: 4,
          border: `1px solid var(--node-operator)50`, whiteSpace: 'nowrap',
          pointerEvents: 'none',
        }}>
          EDIT CODE ↑
        </span>
      )}
    </div>
  );
}

/* ═══════════════════════════════════════════════════════════
   CODE EDITOR — Python syntax highlight, no external lib
   Step 6: slides up from bottom, 50% height
   ═══════════════════════════════════════════════════════════ */

const PY_KEYWORDS = /\b(def|return|if|elif|else|for|while|in|not|and|or|True|False|None|import|from|as|class|pass|break|continue|try|except|finally|with|yield|lambda|del|global|nonlocal|raise|assert)\b/g;
const PY_STRINGS  = /("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|"""[\s\S]*?"""|'''[\s\S]*?''')/g;
const PY_COMMENTS = /(#[^\n]*)/g;
const PY_NUMBERS  = /\b(\d+\.?\d*)\b/g;
const PY_FUNCS    = /\b([a-zA-Z_]\w*)\s*(?=\()/g;
const PY_DECORATORS = /(@[a-zA-Z_]\w*)/g;

function highlightPython(code) {
  // Build HTML string — escape first, then inject spans
  const esc = (s) => s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
  let result = '';
  let i = 0;

  // Tokenise line by line to handle comments correctly
  const lines = code.split('\n');
  return lines.map(line => {
    // Tokenise: strings > comments > keywords > numbers > funcs > decorators > plain
    let out = '';
    let rest = line;
    let col = 0;

    // Collect tokens with position
    const tokens = [];

    // Strings (including triple-quoted, handled per line for simplicity)
    let m;
    const str_re = /("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')/g;
    str_re.lastIndex = 0;
    while ((m = str_re.exec(rest)) !== null) {
      tokens.push({ start: m.index, end: m.index + m[0].length, type: 'string', val: m[0] });
    }

    // Comments
    const com_re = /(#[^\n]*)/g;
    com_re.lastIndex = 0;
    while ((m = com_re.exec(rest)) !== null) {
      if (!tokens.some(t => m.index >= t.start && m.index < t.end)) {
        tokens.push({ start: m.index, end: m.index + m[0].length, type: 'comment', val: m[0] });
      }
    }

    // Keywords
    const kw_re = /\b(def|return|if|elif|else|for|while|in|not|and|or|True|False|None|import|from|as|class|pass|break|continue|try|except|finally|with|yield|lambda|del|global|nonlocal|raise|assert)\b/g;
    kw_re.lastIndex = 0;
    while ((m = kw_re.exec(rest)) !== null) {
      if (!tokens.some(t => m.index >= t.start && m.index < t.end)) {
        tokens.push({ start: m.index, end: m.index + m[0].length, type: 'keyword', val: m[0] });
      }
    }

    // Numbers
    const num_re = /\b(\d+\.?\d*)\b/g;
    num_re.lastIndex = 0;
    while ((m = num_re.exec(rest)) !== null) {
      if (!tokens.some(t => m.index >= t.start && m.index < t.end)) {
        tokens.push({ start: m.index, end: m.index + m[0].length, type: 'number', val: m[0] });
      }
    }

    // Functions (word before open paren)
    const fn_re = /\b([a-zA-Z_]\w*)\s*(?=\()/g;
    fn_re.lastIndex = 0;
    while ((m = fn_re.exec(rest)) !== null) {
      if (!tokens.some(t => m.index >= t.start && m.index < t.end)) {
        tokens.push({ start: m.index, end: m.index + m[1].length, type: 'func', val: m[1] });
      }
    }

    // Sort by start pos
    tokens.sort((a, b) => a.start - b.start);

    let pos = 0;
    const COLOR = {
      keyword: '#e8a020',
      string:  '#2ab8a0',
      comment: '#565250',
      number:  '#9a7fb8',
      func:    '#5090e0',
    };
    for (const t of tokens) {
      if (t.start < pos) continue;
      out += esc(rest.slice(pos, t.start));
      out += `<span style="color:${COLOR[t.type]}">${esc(t.val)}</span>`;
      pos = t.end;
    }
    out += esc(rest.slice(pos));
    return out;
  }).join('\n');
}

const DEFAULT_OPERATOR_CODE = `# Operator: budget.allocate
# Sustena · homestead.bonnie
# Version: 2.1.0

def execute(state, inputs):
    """Allocate funds to a pocket."""
    pocket_name = inputs["pocket_name"]
    amount = inputs["amount"]
    period = inputs.get("period", "monthly")

    # Constraint checks (pre-execution)
    cash = state.finances.cash_position
    if cash < amount:
        raise ConstraintError("balance_constraint", cash, amount)

    allocated = state.finances.pockets.get(pocket_name, 0)
    total = sum(state.finances.pockets.values())

    if total + amount > state.income.monthly:
        raise ConstraintError("sum_constraint", total, amount)

    # Apply delta
    delta = {
        f"finances.pockets.{pocket_name}": allocated + amount,
        "finances.cash_position": cash - amount,
    }

    # Emit event
    events = [
        Event("BUDGET_ALLOCATED", {
            "pocket": pocket_name,
            "amount": amount,
            "period": period,
        })
    ]

    return delta, events
`;

function CodeEditor({ operatorNode, onClose }) {
  const [code, setCode] = dUseState(DEFAULT_OPERATOR_CODE);
  const [saved, setSaved] = dUseState(false);
  const [exiting, setExiting] = dUseState(false);
  const textareaRef = dUseRef(null);
  const lineCount = code.split('\n').length;

  const handleKeyDown = (e) => {
    if (e.key === 'Tab') {
      e.preventDefault();
      const start = e.target.selectionStart;
      const end   = e.target.selectionEnd;
      const newCode = code.slice(0, start) + '    ' + code.slice(end);
      setCode(newCode);
      setTimeout(() => {
        if (textareaRef.current) {
          textareaRef.current.selectionStart = start + 4;
          textareaRef.current.selectionEnd   = start + 4;
        }
      }, 0);
    }
  };

  const handleSave = () => {
    setSaved(true);
    window.flash?.(`${operatorNode?.label || 'Operator'} saved`, 'ok');
    setTimeout(() => setSaved(false), 2000);
  };

  const handleClose = () => {
    setExiting(true);
    setTimeout(onClose, 220);
  };

  return (
    <div style={{
      position: 'absolute', left: 0, right: 0, bottom: 0,
      height: '50%',
      background: 'var(--bg-base)',
      borderTop: '2px solid var(--node-operator)',
      zIndex: 50,
      display: 'flex', flexDirection: 'column',
      transform: exiting ? 'translateY(100%)' : 'translateY(0)',
      transition: 'transform 0.22s cubic-bezier(0.22,0.61,0.36,1)',
      animation: 'codeEditorUp 0.25s cubic-bezier(0.22,0.61,0.36,1)',
      boxShadow: '0 -8px 32px rgba(0,0,0,0.5)',
    }}>
      <style>{`
        @keyframes codeEditorUp {
          from { transform: translateY(100%); }
          to   { transform: translateY(0); }
        }
      `}</style>

      {/* Header */}
      <div style={{
        padding: '8px 14px', flexShrink: 0,
        borderBottom: '1px solid var(--border)',
        background: 'var(--bg-surface)',
        display: 'flex', alignItems: 'center', gap: 10,
      }}>
        <span style={{
          width: 8, height: 8, borderRadius: '50%',
          background: 'var(--node-operator)', flexShrink: 0,
        }} />
        <span style={{ fontFamily: 'var(--mono)', fontSize: 12, fontWeight: 500, color: 'var(--text-primary)', flex: 1 }}>
          {operatorNode?.label || 'operator'}.py
        </span>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>OPERATOR · PYTHON</span>
        <div style={{ display: 'flex', gap: 6 }}>
          <button onClick={() => window.flash?.('Test run — coming in v2', 'info')} style={{
            padding: '4px 10px',
            fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.06em', textTransform: 'uppercase',
            color: 'var(--text-muted)', background: 'transparent',
            border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
            transition: 'all var(--t-fast)',
          }}
          onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--teal-border)'; e.currentTarget.style.color = 'var(--teal)'; }}
          onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border-mid)'; e.currentTarget.style.color = 'var(--text-muted)'; }}
          >RUN TEST</button>
          <button onClick={() => { setCode(DEFAULT_OPERATOR_CODE); window.flash?.('Code reset', 'info'); }} style={{
            padding: '4px 10px',
            fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.06em', textTransform: 'uppercase',
            color: 'var(--text-muted)', background: 'transparent',
            border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
            transition: 'all var(--t-fast)',
          }}>RESET</button>
          <button onClick={handleSave} style={{
            padding: '4px 10px',
            fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.06em', textTransform: 'uppercase',
            color: saved ? 'var(--bg-base)' : 'var(--bg-base)',
            background: saved ? 'var(--ok)' : 'var(--node-operator)',
            border: `1px solid ${saved ? 'var(--ok)' : 'var(--node-operator)'}`,
            borderRadius: 'var(--radius-sm)',
            transition: 'all var(--t-fast)',
          }}>{saved ? '✓ SAVED' : 'SAVE OPERATOR'}</button>
          <button onClick={handleClose} style={{ color: 'var(--text-muted)', padding: 4 }}>
            <Icon name="x" size={12} />
          </button>
        </div>
      </div>

      {/* Editor body */}
      <div style={{ flex: 1, minHeight: 0, display: 'flex', overflow: 'hidden' }}>
        {/* Line numbers */}
        <div style={{
          padding: '10px 0',
          width: 44, flexShrink: 0,
          background: 'var(--bg-surface)',
          borderRight: '1px solid var(--border)',
          overflowY: 'hidden',
          display: 'flex', flexDirection: 'column',
          alignItems: 'flex-end',
        }}>
          {Array.from({ length: lineCount }).map((_, i) => (
            <div key={i} style={{
              fontFamily: 'var(--mono)', fontSize: 12, lineHeight: '20px',
              color: 'var(--text-dim)', paddingRight: 8, minWidth: 36, textAlign: 'right',
              userSelect: 'none',
            }}>{i + 1}</div>
          ))}
        </div>

        {/* Syntax-highlighted display layer */}
        <div style={{ flex: 1, position: 'relative', overflow: 'hidden' }}>
          {/* Highlighted (read layer) */}
          <pre
            aria-hidden="true"
            style={{
              position: 'absolute', inset: 0,
              padding: '10px 14px',
              fontFamily: 'var(--mono)', fontSize: 12, lineHeight: '20px',
              color: 'var(--text-secondary)',
              pointerEvents: 'none', zIndex: 1,
              overflow: 'hidden',
              whiteSpace: 'pre',
              margin: 0,
              background: 'transparent',
            }}
            dangerouslySetInnerHTML={{ __html: highlightPython(code) }}
          />
          {/* Actual textarea (transparent text for caret only) */}
          <textarea
            ref={textareaRef}
            value={code}
            onChange={e => setCode(e.target.value)}
            onKeyDown={handleKeyDown}
            spellCheck={false}
            style={{
              position: 'absolute', inset: 0,
              padding: '10px 14px',
              fontFamily: 'var(--mono)', fontSize: 12, lineHeight: '20px',
              color: 'transparent',
              caretColor: 'var(--amber)',
              background: 'transparent',
              border: 'none', outline: 'none', resize: 'none',
              zIndex: 2,
              whiteSpace: 'pre',
              overflow: 'auto',
            }}
          />
        </div>
      </div>
    </div>
  );
}

/* ═══════════════════════════════════════════════════════════
   ORCHIE EXPANSION PANEL — Step 4
   Slides in from right ~40% width when Orchie node clicked
   Top: intent cards with confidence bars
   Bottom: ambient activity animation
   ═══════════════════════════════════════════════════════════ */

const ORCHIE_INTENTS = [
  { text: 'Complete Vyyb Phase 1 setup',     confidence: 87, source: 'from task list',  id: 'i1' },
  { text: 'Review morning briefing',          confidence: 92, source: 'from calendar',  id: 'i2' },
  { text: 'Schedule delivery dispatch',       confidence: 74, source: 'from chat',      id: 'i3' },
  { text: 'File Sustena XII entity docs',     confidence: 61, source: 'from task list', id: 'i4' },
];

/*
  OrchieExpansionPanel — full-page overlay (like Monitor/Simulator panels).
  Opens when user clicks the Orchie node in the DAG.
  Top: voice + chat CTAs (primary interaction entry points).
  Below: intent cards, customisations, ambient activity viz.
  Renders at fixed inset:0 z-index:500 so it covers the full app view.
*/
function OrchieExpansionPanel({ open, onClose, tick = 0 }) {
  const [intents, setIntents] = dUseState(ORCHIE_INTENTS);
  const [prefResponseLen, setPrefResponseLen] = dUseState('balanced');
  const [prefProactive, setPrefProactive] = dUseState(true);
  const [prefNotifs, setPrefNotifs] = dUseState('important');
  const [isRecording, setIsRecording] = dUseState(false);
  const [chatInput, setChatInput] = dUseState('');

  const dismissIntent = (id) => setIntents(prev => prev.filter(i => i.id !== id));

  const handleVoice = () => {
    setIsRecording(r => {
      if (!r) window.flash?.('Listening… speak now', 'info');
      else     window.flash?.('Voice captured — Orchie is processing', 'ok');
      return !r;
    });
  };

  const handleChat = (e) => {
    e.preventDefault();
    if (!chatInput.trim()) return;
    window.flash?.(`Orchie: processing "${chatInput.slice(0, 40)}…"`, 'info');
    setChatInput('');
  };

  return (
    /* Full-page overlay — same z-level as Monitor/Simulator */
    <div style={{
      position: 'fixed', inset: 0,
      zIndex: 500,
      background: 'var(--bg-base)',
      display: 'flex', flexDirection: 'column',
      opacity: open ? 1 : 0,
      pointerEvents: open ? 'all' : 'none',
      transition: 'opacity 0.22s ease',
    }}>
      {/* ── Top bar: back arrow + ORCHIE label + status ── */}
      <div style={{
        padding: '10px 16px', flexShrink: 0,
        borderBottom: '1px solid var(--border)',
        display: 'flex', alignItems: 'center', gap: 10,
        background: 'var(--bg-surface)',
      }}>
        {/* Back button */}
        <button onClick={onClose} style={{
          display: 'flex', alignItems: 'center', gap: 5,
          color: 'var(--text-secondary)', padding: '3px 8px',
          borderRadius: 'var(--radius-sm)',
          border: '1px solid var(--border)', background: 'transparent',
          transition: 'all var(--t-fast)', cursor: 'pointer',
          fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.07em',
        }}
          onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
          onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
        >
          <svg width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
            <path d="M6 1.5L2.5 4.5L6 7.5" />
          </svg>
          BACK
        </button>

        <div style={{ width: 1, height: 16, background: 'var(--border)', flexShrink: 0 }} />

        {/* Orchie monogram */}
        <div style={{
          width: 22, height: 22, borderRadius: '50%',
          background: 'var(--bg-base)', border: '1px solid var(--amber-border)',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          boxShadow: '0 0 6px var(--amber-glow)', flexShrink: 0,
        }}>
          <SustenaMark size={13} />
        </div>

        <span style={{
          fontFamily: 'var(--mono)', fontSize: 14, fontWeight: 600,
          color: 'var(--text-primary)', letterSpacing: '0.08em',
        }}>
          ORCHIE
        </span>

        <span style={{ flex: 1 }} />

        {/* Tick-driven state label */}
        <span style={{
          fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)',
          letterSpacing: '0.07em', textTransform: 'uppercase',
        }}>
          {['DELEGATING', 'RECEIVING', 'ANSWERING', 'VOTING'][tick % 4]}
        </span>

        {/* ACTIVE pill */}
        <span style={{
          display: 'flex', alignItems: 'center', gap: 5,
          padding: '3px 8px',
          background: '#2ab8a015', border: '1px solid #2ab8a040',
          borderRadius: 'var(--radius-sm)',
        }}>
          <span className="pulse" style={{
            width: 5, height: 5, borderRadius: '50%',
            background: 'var(--teal)', flexShrink: 0,
          }} />
          <span style={{
            fontFamily: 'var(--mono)', fontSize: 9,
            color: 'var(--teal)', letterSpacing: '0.08em',
          }}>ACTIVE</span>
        </span>
      </div>

      {/* ── Primary CTAs: Voice (top) + Chat (below) ── */}
      <div style={{
        padding: '24px 24px 18px',
        borderBottom: '1px solid var(--border)',
        display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 16,
        flexShrink: 0,
        background: 'linear-gradient(180deg, var(--bg-surface) 0%, var(--bg-base) 100%)',
      }}>
        {/* Voice CTA — primary, large mic button */}
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 10 }}>
          <button onClick={handleVoice} style={{
            width: 72, height: 72, borderRadius: '50%',
            background: isRecording
              ? 'radial-gradient(circle at 50% 50%, #e0505022 0%, transparent 70%)'
              : 'radial-gradient(circle at 50% 50%, var(--amber-glow) 0%, transparent 70%)',
            border: isRecording
              ? '2px solid var(--danger)'
              : '2px solid var(--amber-border)',
            boxShadow: isRecording
              ? '0 0 0 6px #e0505018, 0 0 0 14px #e0505008'
              : '0 0 0 4px var(--amber-glow), 0 4px 18px rgba(0,0,0,0.4)',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            cursor: 'pointer', transition: 'all 0.25s ease',
            animation: isRecording ? 'orchMicPulse 1.1s ease-in-out infinite' : 'none',
          }}>
            <style>{`
              @keyframes orchMicPulse {
                0%,100% { box-shadow: 0 0 0 4px #e0505025, 0 0 0 10px #e0505010; }
                50%      { box-shadow: 0 0 0 10px #e0505035, 0 0 0 20px #e0505015; }
              }
            `}</style>
            {/* Microphone SVG icon */}
            <svg width="28" height="28" viewBox="0 0 24 24" fill="none"
              stroke={isRecording ? 'var(--danger)' : 'var(--amber)'}
              strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"
            >
              <rect x="9" y="2" width="6" height="12" rx="3" />
              <path d="M5 10a7 7 0 0 0 14 0" />
              <line x1="12" y1="17" x2="12" y2="21" />
              <line x1="9" y1="21" x2="15" y2="21" />
            </svg>
          </button>
          <span style={{
            fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.1em',
            textTransform: 'uppercase',
            color: isRecording ? 'var(--danger)' : 'var(--amber)',
          }}>
            {isRecording ? '● LISTENING…' : 'SPEAK TO ORCHIE'}
          </span>
        </div>

        {/* Chat CTA — text input row */}
        <form onSubmit={handleChat} style={{ display: 'flex', gap: 8, width: '100%', maxWidth: 480 }}>
          <input
            type="text"
            value={chatInput}
            onChange={e => setChatInput(e.target.value)}
            placeholder="Ask Orchie anything…"
            style={{
              flex: 1, padding: '9px 13px',
              fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-primary)',
              background: 'var(--bg-base)',
              border: '1px solid var(--border)',
              borderRadius: 'var(--radius-md)',
              outline: 'none', transition: 'border-color var(--t-fast)',
            }}
            onFocus={e => e.target.style.borderColor = 'var(--amber-border)'}
            onBlur={e => e.target.style.borderColor = 'var(--border)'}
          />
          <button type="submit" style={{
            padding: '9px 16px', flexShrink: 0,
            background: chatInput.trim() ? 'var(--amber)' : 'var(--bg-overlay)',
            border: `1px solid ${chatInput.trim() ? 'var(--amber)' : 'var(--border)'}`,
            borderRadius: 'var(--radius-md)',
            color: chatInput.trim() ? 'var(--bg-base)' : 'var(--text-dim)',
            fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.07em',
            transition: 'all var(--t-fast)', cursor: 'pointer',
          }}>
            SEND
          </button>
        </form>
      </div>

      <div style={{ flex: 1, overflowY: 'auto', display: 'flex', flexDirection: 'column', gap: 0 }}>

        {/* ── TOP HALF: User intent cards ── */}
        <div style={{ padding: '14px 14px 10px', borderBottom: '1px solid var(--border)' }}>
          <span className="label-10" style={{ display: 'block', marginBottom: 10 }}>ORCHIE KNOWS YOU WANT</span>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            {intents.map(intent => (
              <div key={intent.id} className="fade-up" style={{
                padding: '10px 12px',
                background: 'var(--bg-base)', border: '1px solid var(--border)',
                borderRadius: 'var(--radius-md)',
                position: 'relative',
              }}>
                {/* Dismiss */}
                <button onClick={() => dismissIntent(intent.id)} style={{
                  position: 'absolute', top: 8, right: 8,
                  color: 'var(--text-dim)', padding: 2,
                  transition: 'color var(--t-fast)',
                }}
                onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
                onMouseLeave={e => e.currentTarget.style.color = 'var(--text-dim)'}
                >
                  <Icon name="x" size={10} />
                </button>
                <p style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-primary)', lineHeight: 1.4, marginBottom: 8, paddingRight: 16 }}>
                  {intent.text}
                </p>
                {/* Confidence bar */}
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <div style={{ flex: 1, height: 3, background: 'var(--bg-raised)', borderRadius: 2, overflow: 'hidden' }}>
                    <div style={{
                      width: `${intent.confidence}%`, height: '100%',
                      background: intent.confidence > 80 ? 'var(--teal)' : intent.confidence > 60 ? 'var(--amber)' : 'var(--text-muted)',
                      borderRadius: 2, transition: 'width 0.6s ease',
                    }} />
                  </div>
                  <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)', minWidth: 30 }}>{intent.confidence}%</span>
                </div>
                <span className="meta-10" style={{ fontSize: 9, color: 'var(--text-dim)', marginTop: 4, display: 'block' }}>{intent.source}</span>
              </div>
            ))}
            {intents.length === 0 && (
              <div style={{ textAlign: 'center', padding: '16px 0' }}>
                <span className="meta-11" style={{ color: 'var(--text-dim)' }}>All intents reviewed</span>
              </div>
            )}
          </div>
        </div>

        {/* ── Customisations ── */}
        <div style={{ padding: '12px 14px', borderBottom: '1px solid var(--border)' }}>
          <span className="label-10" style={{ display: 'block', marginBottom: 10 }}>CUSTOMISATIONS</span>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            {/* Response length */}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
              <span className="meta-10" style={{ color: 'var(--text-muted)' }}>Response length</span>
              <div style={{ display: 'flex', gap: 3 }}>
                {['brief', 'balanced', 'detailed'].map(v => (
                  <button key={v} onClick={() => setPrefResponseLen(v)} style={{
                    flex: 1, padding: '4px 0',
                    fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.06em', textTransform: 'uppercase',
                    color: prefResponseLen === v ? 'var(--amber)' : 'var(--text-muted)',
                    background: prefResponseLen === v ? 'var(--amber-glow)' : 'transparent',
                    border: `1px solid ${prefResponseLen === v ? 'var(--amber-border)' : 'var(--border)'}`,
                    borderRadius: 'var(--radius-sm)', transition: 'all var(--t-fast)',
                  }}>{v}</button>
                ))}
              </div>
            </div>
            {/* Proactive suggestions */}
            <OrchieToggle label="Proactive suggestions" value={prefProactive} onChange={setPrefProactive} />
            {/* Notification level */}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
              <span className="meta-10" style={{ color: 'var(--text-muted)' }}>Notification level</span>
              <div style={{ display: 'flex', gap: 3 }}>
                {['all', 'important', 'silent'].map(v => (
                  <button key={v} onClick={() => setPrefNotifs(v)} style={{
                    flex: 1, padding: '4px 0',
                    fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.06em', textTransform: 'uppercase',
                    color: prefNotifs === v ? 'var(--amber)' : 'var(--text-muted)',
                    background: prefNotifs === v ? 'var(--amber-glow)' : 'transparent',
                    border: `1px solid ${prefNotifs === v ? 'var(--amber-border)' : 'var(--border)'}`,
                    borderRadius: 'var(--radius-sm)', transition: 'all var(--t-fast)',
                  }}>{v}</button>
                ))}
              </div>
            </div>
          </div>
        </div>

        {/* ── BOTTOM HALF: Activity visualisation ── */}
        <div style={{ padding: '12px 14px', flex: 1 }}>
          <span className="label-10" style={{ display: 'block', marginBottom: 10 }}>ACTIVITY · AMBIENT</span>
          <OrchieActivityViz tick={tick} />
        </div>
      </div>
    </div>
  );
}

function OrchieToggle({ label, value, onChange }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10 }}>
      <span style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-secondary)' }}>{label}</span>
      <button onClick={() => onChange(!value)} style={{
        width: 32, height: 18, borderRadius: 9, flexShrink: 0,
        background: value ? 'var(--amber)' : 'var(--bg-overlay)',
        border: `1px solid ${value ? 'var(--amber)' : 'var(--border-mid)'}`,
        position: 'relative', transition: 'all var(--t-mid)', cursor: 'pointer',
      }}>
        <span style={{
          position: 'absolute', top: 2,
          left: value ? 'calc(100% - 16px)' : 2,
          width: 12, height: 12, borderRadius: 6,
          background: value ? 'var(--bg-base)' : 'var(--text-dim)',
          transition: 'left var(--t-mid)',
        }} />
      </button>
    </div>
  );
}

/* Ambient activity SVG animation — 4 states cycling via CSS */
function OrchieActivityViz({ tick }) {
  // States: delegating, receiving, querying, voting
  const state = tick % 4;
  const nodes = [
    { label: 'ORCHIE',   x: 90, y: 80, isCenter: true },
    { label: 'budget.op', x: 30, y: 40  },
    { label: 'pantry.op', x: 155, y: 40 },
    { label: 'COUNCIL',   x: 90, y: 140 },
    { label: 'INBOX',     x: 30, y: 130 },
  ];

  // Animated dot positions — different per state
  const dotPath = {
    0: [{ from: 0, to: 1 }, { from: 0, to: 2 }], // delegating
    1: [{ from: 1, to: 0 }, { from: 2, to: 0 }], // receiving feedback
    2: [{ from: 4, to: 0 }],                       // answering query
    3: [{ from: 0, to: 3 }, { from: 3, to: 0 }],  // council voting
  }[state] || [];

  const stateLabels = ['DELEGATING', 'RECEIVING', 'ANSWERING', 'VOTING'];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      {/* State label */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span className="pulse" style={{ width: 5, height: 5, borderRadius: '50%', background: 'var(--amber)', flexShrink: 0 }} />
        <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--amber)', letterSpacing: '0.08em' }}>{stateLabels[state]}</span>
      </div>

      {/* SVG ambient graph */}
      <svg width="100%" height="180" viewBox="0 0 190 180" style={{ overflow: 'visible' }}>
        <style>{`
          @keyframes orchDot {
            0%   { transform: translate(0px, 0px); opacity: 0; }
            10%  { opacity: 1; }
            90%  { opacity: 1; }
            100% { transform: translate(var(--dx), var(--dy)); opacity: 0; }
          }
          @keyframes orchPulse {
            0%, 100% { r: 9; opacity: 0.7; }
            50%       { r: 13; opacity: 1; }
          }
        `}</style>

        {/* Edges */}
        {[
          [0,1],[0,2],[0,3],[0,4]
        ].map(([a,b], i) => (
          <line key={i}
            x1={nodes[a].x} y1={nodes[a].y}
            x2={nodes[b].x} y2={nodes[b].y}
            stroke="var(--border-mid)" strokeWidth="0.8" opacity="0.4"
          />
        ))}

        {/* Nodes */}
        {nodes.map((n, i) => (
          <g key={i}>
            {n.isCenter ? (
              <>
                <circle cx={n.x} cy={n.y} r="14"
                  fill="var(--amber-glow)" stroke="var(--amber-border)" strokeWidth="1"
                  style={{ animation: 'orchPulse 2s ease-in-out infinite' }}
                />
                <g transform={`translate(${n.x-7}, ${n.y-7})`}>
                  <SustenaMark size={14} primary="var(--amber)" secondary="var(--text-dim)" />
                </g>
              </>
            ) : (
              <>
                <circle cx={n.x} cy={n.y} r="7"
                  fill="var(--bg-raised)" stroke="var(--border-mid)" strokeWidth="1"
                />
                <circle cx={n.x} cy={n.y} r="3"
                  fill="var(--text-dim)"
                />
              </>
            )}
            <text x={n.x} y={n.y + (n.isCenter ? 24 : 18)}
              textAnchor="middle"
              fontFamily="DM Mono" fontSize="8"
              fill="var(--text-muted)"
            >{n.label}</text>
          </g>
        ))}

        {/* Animated flow dots */}
        {dotPath.map((dp, i) => {
          const from = nodes[dp.from];
          const to = nodes[dp.to];
          const dx = to.x - from.x;
          const dy = to.y - from.y;
          return (
            <circle key={`dot-${i}`}
              cx={from.x} cy={from.y} r="3"
              fill="var(--amber)"
              style={{
                '--dx': `${dx}px`, '--dy': `${dy}px`,
                animation: `orchDot 1.8s ease-in-out ${i * 0.4}s infinite`,
                transformOrigin: `${from.x}px ${from.y}px`,
              }}
            />
          );
        })}

        {/* Center pulse ring on query/vote */}
        {(state === 2 || state === 3) && (
          <circle cx={nodes[0].x} cy={nodes[0].y} r="20"
            fill="none" stroke="var(--amber)" strokeWidth="0.8" opacity="0.4"
            style={{ animation: 'orchPulse 1.4s ease-in-out infinite' }}
          />
        )}
      </svg>

      {/* Mini to-do checklist (ambient) */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
        <span className="label-10" style={{ fontSize: 9 }}>CURRENT TASKS</span>
        {[
          { done: true,              label: 'Morning briefing compiled' },
          { done: true,              label: 'Council vote SUS-0148 tracked' },
          { done: false, active: true, label: 'Fetching pantry state' },
          { done: false, active: false, label: 'Awaiting M-Pesa confirm' },
        ].map((t, i) => (
          <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
            <span style={{
              fontFamily: 'var(--mono)', fontSize: 10,
              color: t.done ? 'var(--ok)' : t.active ? 'var(--amber)' : 'var(--text-dim)',
            }}>
              {t.done ? '✓' : t.active ? '⟳' : '○'}
            </span>
            <span style={{
              fontFamily: 'var(--ui)', fontSize: 11,
              color: t.done ? 'var(--text-muted)' : t.active ? 'var(--text-primary)' : 'var(--text-dim)',
              textDecoration: t.done ? 'line-through' : 'none',
            }}>
              {t.label}
            </span>
            {t.active && (
              <span className="pulse" style={{ width: 4, height: 4, borderRadius: '50%', background: 'var(--amber)', marginLeft: 2 }} />
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

/* ═══════════════════════════════════════════════════════════
   DATA MODELS
   ═══════════════════════════════════════════════════════════ */

/* Sustains */
const SUSTAINS = [
  { id: 'homestead.bonnie', label: 'Homestead',    sub: 'bonnie',    status: 'live' },
  { id: 'vyyb.hive',        label: 'Vyyb Hive',    sub: 'hive',      status: 'live' },
  { id: 'mkulima.alpha',    label: 'Mkulima',      sub: 'alpha',     status: 'seed' },
  { id: 'sustena.xii',      label: 'Sustena XII',  sub: 'core',      status: 'live' },
  { id: 'chama.nairobi',    label: 'Chama Nairobi',sub: 'circle',    status: 'seed' },
];

/* State tree — used by monitor */
const STATE_TREE = [
  { path: 'finances.cash_position',    value: 184250,  target: 200000, fmt: 'ksh', cstr: 'ok',    desc: 'Consolidated cash' },
  { path: 'finances.pockets.food',     value: 8420,    target: 10000,  fmt: 'ksh', cstr: 'amber', desc: 'Food pocket' },
  { path: 'finances.pockets.transport',value: 4100,    target: 5000,   fmt: 'ksh', cstr: 'ok',    desc: 'Transport' },
  { path: 'finances.burn_rate',        value: 4214,    target: 3500,   fmt: 'ksh', cstr: 'amber', desc: 'Avg KSH/day', lowerBetter: true },
  { path: 'pantry.cooking_oil_L',      value: 0.4,     target: 5.0,    fmt: 'L',   cstr: 'red',   desc: 'Critically low' },
  { path: 'pantry.tomatoes_kg',        value: 2.8,     target: 4.0,    fmt: 'kg',  cstr: 'ok',    desc: 'Fresh produce' },
  { path: 'system.pawa_balance',       value: 8420,    target: 10000,  fmt: 'pwa', cstr: 'ok',    desc: 'Orchie tokens' },
  { path: 'chama.contributions',       value: 198400,  target: 250000, fmt: 'ksh', cstr: 'ok',    desc: 'Pool balance' },
  { path: 'system.api_p95_ms',         value: 428,     target: 400,    fmt: 'ms',  cstr: 'ok',    desc: 'Haiku inference', lowerBetter: true },
  { path: 'council.quorum_pct',        value: 78,      target: 80,     fmt: '%',   cstr: 'amber', desc: 'Near threshold' },
];

/* Operatives */
const OPERATIVES = [
  {
    id: 'op-mentor', name: 'Mentor', role: 'Strategic advisor · finance',
    status: 'active', confidence: 88, pawa: 42, task: 'Monitoring burn rate deviation from weekly plan. Flagged 3 anomalies.',
    subtasks: [
      { name: 'Analyse burn trajectory', done: true, progress: 100 },
      { name: 'Draft anomaly report', done: false, progress: 62 },
      { name: 'Propose reallocation', done: false, progress: 0 },
    ],
  },
  {
    id: 'op-curator', name: 'Curator', role: 'Pantry & procurement',
    status: 'alert', confidence: 71, pawa: 18, task: 'Cooking oil at critical threshold. Awaiting batch-order approval from Bonnie.',
    subtasks: [
      { name: 'Detect pantry breach', done: true, progress: 100 },
      { name: 'Source vendor · Mama Mboga', done: true, progress: 100 },
      { name: 'Await approval', done: false, progress: 35 },
    ],
  },
  {
    id: 'op-navigator', name: 'Navigator', role: 'Council & governance',
    status: 'active', confidence: 94, pawa: 55, task: 'Tracking SUS-0148 quorum. 1h 22m to close. Drafted summary for Bonnie.',
    subtasks: [
      { name: 'Monitor vote SUS-0148', done: false, progress: 78 },
      { name: 'Draft vote brief', done: true, progress: 100 },
    ],
  },
  {
    id: 'op-protege', name: 'Protégé', role: 'Learning · pattern recognition',
    status: 'idle', confidence: 65, pawa: 8, task: 'Idle — observing Mentor\'s burn analysis to refine own models.',
    subtasks: [
      { name: 'Shadow Mentor', done: false, progress: 0 },
    ],
  },
];

/* Sim nodes — updated with Orchie and improved labelling */
const SIM_NODES = [
  { id: 'state0',  type: 'state',      label: 'finances.cash',     x: 30,  y: 40  },
  { id: 'state1',  type: 'state',      label: 'pantry.items',      x: 30,  y: 130 },
  { id: 'state2',  type: 'state',      label: 'finances.burn',     x: 30,  y: 220 },
  { id: 'gate0',   type: 'gate',       label: 'entry gate',        x: 30,  y: 310 },
  { id: 'op0',     type: 'operator',   label: 'mpesa.parse',       x: 200, y: 40  },
  { id: 'op1',     type: 'operator',   label: 'budget.allocate',   x: 200, y: 130 },
  { id: 'op2',     type: 'operator',   label: 'pantry.consume',    x: 200, y: 220 },
  { id: 'cstr0',   type: 'constraint', label: 'sum_constraint',    x: 380, y: 80  },
  { id: 'cstr1',   type: 'constraint', label: 'balance_check',     x: 380, y: 200 },
  { id: 'orchie',  type: 'operative',  label: 'ORCHIE',            x: 560, y: 130 },
  { id: 'ev0',     type: 'event',      label: 'BUDGET_ALLOCATED',  x: 720, y: 60  },
  { id: 'ev1',     type: 'event',      label: 'BURN_RATE_ALERT',   x: 720, y: 220 },
];

const SIM_EDGES = [
  ['state0','op0'], ['state1','op2'], ['state2','op2'],
  ['gate0','op1'],
  ['op0','op1'], ['op1','cstr0'], ['op2','cstr1'],
  ['cstr0','orchie'], ['cstr1','orchie'],
  ['orchie','ev0'], ['orchie','ev1'],
];

/* Scenario tree */
const SCENARIO_TREE = [
  { id: 'root', label: 'BASE · homestead.bonnie', score: null, children: [
    { id: 'A', label: 'BRANCH A · nominal', score: 0.81, hot: true, children: [
      { id: 'A1', label: 'A.1 · conserve',   score: 0.79 },
      { id: 'A2', label: 'A.2 · optimise',   score: 0.87, hot: true },
      { id: 'A3', label: 'A.3 · expand',     score: 0.62 },
    ]},
    { id: 'B', label: 'BRANCH B · austerity', score: 0.58, children: [
      { id: 'B1', label: 'B.1 · freeze',     score: 0.51 },
      { id: 'B2', label: 'B.2 · partial',    score: 0.61 },
    ]},
    { id: 'C', label: 'BRANCH C · growth',   score: 0.44 },
    { id: 'D', label: 'BRANCH D · fork-D',   score: 0.68 },
  ]},
];

/* Proposal data */
const PROPOSALS = [
  {
    id: 'SUS-0148', sustain: 'homestead.bonnie',
    title: 'Advance Q3 disbursement to Carbon-R&D by 14 days',
    summary: 'Move the scheduled KSH 380,000 Q3 disbursement from the 28th to the 14th to unblock two grant windows and enable Vyyb Phase 1 procurement. Simulation shows 0.87 outcome score.',
    cta: 'EXECUTE',
    autonomy: 'HIGH',
    cost: '42 pwa',
    sim: { outcomeScore: 0.87, constraintPassRate: 1.0, runs: 100, projection: 'Cash reserves remain 12.4% above floor. Grant unlock by +9 days.' },
    council: { for: 7, against: 2, abstain: 1 },
    status: 'awaiting',
  },
];

/* Event log generator */
const LOG_EVENTS = [
  { sustain: 'homestead',  operator: 'mpesa.parse',      delta: 'KSH 1,120 received · stk-4827', op: 'STATE+',  tone: 'ok' },
  { sustain: 'vyyb.hive',  operator: 'budget.allocate',  delta: 'food ← KSH 2,400',              op: 'ΔSTATE',  tone: 'teal' },
  { sustain: 'homestead',  operator: 'pantry.consume',   delta: 'oil −0.4 L · breach',           op: 'ALERT',   tone: 'danger' },
  { sustain: 'sustena.xii',operator: 'council.vote',     delta: 'SUS-0148 · +1 YES',             op: 'VOTE',    tone: 'amber' },
  { sustain: 'mkulima',    operator: 'field.log',        delta: 'harvest 12kg tomatoes',         op: 'STATE+',  tone: 'ok' },
  { sustain: 'homestead',  operator: 'budget.compute',   delta: 'burn_rate → 4,214 KSH/d',       op: 'ΔSTATE',  tone: 'amber' },
];

function pickEvent(seed) {
  const i = seed % LOG_EVENTS.length;
  const e = LOG_EVENTS[i];
  const now = new Date();
  const pad = n => n.toString().padStart(2, '0');
  return {
    ...e,
    key: seed,
    time: `${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`,
  };
}

/* Library items */
const LIBRARY_ITEMS = {
  operatives: [
    { name: 'Mentor',    author: 'sustena.core', version: '1.4.2', trust: 98, downloads: 4821, pawa: '42 pwa', desc: 'Strategic advisor with access to finance state. Monitors burn rate, drafts council proposals, surfaces anomalies.' },
    { name: 'Curator',   author: 'sustena.core', version: '1.2.0', trust: 95, downloads: 3204, pawa: '18 pwa', desc: 'Pantry and procurement specialist. Manages supplier relationships, batch orders, and restock thresholds.' },
    { name: 'Navigator', author: 'sustena.core', version: '1.3.1', trust: 97, downloads: 2918, pawa: '55 pwa', desc: 'Council and governance operative. Tracks proposals, monitors quorum, drafts council briefs.' },
  ],
  operators: [
    { name: 'budget.allocate', author: 'sustena.core', version: '2.1.0', trust: 99, downloads: 11400, pawa: '0.02 pwa', desc: 'Allocate funds to a named pocket with sum and balance constraints. Emits BUDGET_ALLOCATED.' },
    { name: 'mpesa.parse',     author: 'sustena.core', version: '1.0.8', trust: 98, downloads: 8320,  pawa: '0.01 pwa', desc: 'Parse incoming M-Pesa STK notifications and update cash position state.' },
    { name: 'pantry.consume',  author: 'sustena.core', version: '1.1.2', trust: 97, downloads: 5614,  pawa: '0.01 pwa', desc: 'Record pantry item consumption and trigger threshold alerts.' },
  ],
  spores: [
    { name: 'Homestead',  author: 'sustena.core', version: '2.0.0', trust: 99, downloads: 1240, pawa: 'FREE', desc: 'Complete household sustain template. Includes finance pockets, pantry, M-Pesa hooks, and 4 default operatives.' },
    { name: 'Chama',      author: 'sustena.core', version: '1.3.0', trust: 98, downloads: 882,  pawa: 'FREE', desc: 'Group savings circle sustain. Includes contribution tracking, rotation schedule, and council governance.' },
  ],
  widgets: [
    { name: 'Budget Ring',   author: 'sustena.ui', version: '1.0.2', trust: 92, downloads: 4200, pawa: 'FREE', desc: 'Animated donut chart showing pocket allocation.' },
    { name: 'Burn Gauge',    author: 'sustena.ui', version: '1.1.0', trust: 94, downloads: 3180, pawa: 'FREE', desc: 'Live burn rate gauge with ceiling threshold.' },
    { name: 'Proposal Card', author: 'sustena.ui', version: '1.2.1', trust: 96, downloads: 2450, pawa: 'FREE', desc: 'Council proposal vote card with tally bar.' },
  ],
};

function LibraryMark({ kind, size = 20 }) {
  const label = { operatives: '⬢', operators: '◖◗', spores: '❋', widgets: '⊞' }[kind] || '·';
  return <span style={{ fontSize: size, lineHeight: 1 }}>{label}</span>;
}

function WidgetPreview({ name }) {
  if (name === 'Budget Ring') return <PieWidget data={{ slices: [{ label:'Food',value:38,color:'#E8A020' },{ label:'Travel',value:22,color:'#2ab8a0' },{ label:'Other',value:40,color:'#565250' }], total:'KSH 184k' }} size="sm" />;
  if (name === 'Proposal Card') return <ProposalWidget data={{ id:'SUS-0148',title:'Advance Q3',for:7,against:2,abstain:1,quorum:10,eta:'4h',score:0.84 }} size="sm" />;
  return <div style={{ fontFamily:'var(--mono)', fontSize:10, color:'var(--text-muted)' }}>[{name}]</div>;
}

/* Tweaks helpers (used by mcp-shell) */
function useTweaks(defaults) {
  const [t, setT] = dUseState(defaults);
  const setTweak = (key, val) => setT(prev => ({ ...prev, [key]: val }));
  return [t, setTweak];
}

function formatClock(d) {
  const pad = n => n.toString().padStart(2, '0');
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
}

/* ─── CSS variables for node colours (injected once) ────── */
if (!document.getElementById('sustena-node-vars')) {
  const style = document.createElement('style');
  style.id = 'sustena-node-vars';
  style.textContent = `
    :root {
      --node-state:      #5090e0;
      --node-operator:   #e8a020;
      --node-constraint: #9a7fb8;
      --node-operative:  #2ab8a0;
      --node-event:      #4caf80;
      --node-time:       #e05050;
    }
  `;
  document.head.appendChild(style);
}

Object.assign(window, {
  /* DAG */
  DagNode, DagEdges,
  /* Code editor */
  CodeEditor, highlightPython,
  /* Orchie */
  OrchieExpansionPanel, OrchieActivityViz,
  /* Data */
  SUSTAINS, STATE_TREE, OPERATIVES, PROPOSALS,
  SIM_NODES, SIM_EDGES, SCENARIO_TREE,
  LIBRARY_ITEMS, NODE_STYLES,
  pickEvent, LibraryMark, WidgetPreview,
  useTweaks, formatClock,
});
