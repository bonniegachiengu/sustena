/* Sustena Logo — official mark + variants
   Network node: central dot + three branching arcs (triskele).
   Reads as mycelium spore / DAO governance node / signal beacon. */

const { useState } = React;

/* ─── Geometric primitives ──────────────────────────────────
   viewBox is centered on (0,0) at -100..100 for math clarity.
   Three branches at 120° rotation. Each branch:
   - quadratic bezier comma from center → outer node
   - filled outer node
   Center filled disc sits on top.
*/

function SustenaMark({
  size = 96,
  primary = '#E8A020',      // center node
  secondary = '#e8e4dc',    // arcs + outer nodes
  mono = false,             // if true, primary = secondary
  showGuides = false,
}) {
  const a = mono ? secondary : primary;
  const c = secondary;
  // Path math (units in viewBox -100..100):
  // - center disc r=22
  // - outer node r=11, at radius 78 from center
  // - one branch path defined in canonical orientation (angle 0 = +x axis pointing right):
  //   start at center (0,0), arc out to angle ~28° at radius 78
  //   end point: (78·cos28°, 78·sin28°) ≈ (68.86, 36.62)
  //   control point: (52, 0) — pulls tangent along +x then bends down
  const endX = 78 * Math.cos((28 * Math.PI) / 180);
  const endY = 78 * Math.sin((28 * Math.PI) / 180);
  const ctrlX = 52;
  const ctrlY = 0;

  const branchPath = `M 0 0 Q ${ctrlX} ${ctrlY} ${endX} ${endY}`;

  // Branches rotated to point up, down-right, down-left
  // Start at -90° (up) and rotate by 120°
  const rotations = [-90, 30, 150];

  return (
    <svg
      width={size}
      height={size}
      viewBox="-100 -100 200 200"
      style={{ display: 'block' }}
    >
      {showGuides && (
        <g stroke="#3a3a3a" strokeWidth="0.5" fill="none">
          <circle cx="0" cy="0" r="78" strokeDasharray="2 3" />
          <circle cx="0" cy="0" r="22" strokeDasharray="2 3" />
          <line x1="-100" y1="0" x2="100" y2="0" />
          <line x1="0" y1="-100" x2="0" y2="100" />
          {rotations.map(r => (
            <line key={r}
              x1="0" y1="0"
              x2={100 * Math.cos((r * Math.PI) / 180)}
              y2={100 * Math.sin((r * Math.PI) / 180)}
              strokeDasharray="1 2"
            />
          ))}
        </g>
      )}
      {/* Three branches with terminal nodes */}
      {rotations.map(r => (
        <g key={r} transform={`rotate(${r})`}>
          <path
            d={branchPath}
            stroke={c}
            strokeWidth="14"
            fill="none"
            strokeLinecap="round"
          />
          <circle cx={endX} cy={endY} r="11" fill={c} />
        </g>
      ))}
      {/* Center disc */}
      <circle cx="0" cy="0" r="22" fill={a} />
    </svg>
  );
}

/* Wordmark: SUSTENA in DM Mono 500, 0.08em tracking, uppercase.
   First letter S in amber, rest in text-primary. */
function SustenaWordmark({ size = 18, mono = false, light = false }) {
  const restColor = light ? '#0f0f0f' : '#e8e4dc';
  const sColor = mono ? restColor : '#E8A020';
  return (
    <span style={{
      fontFamily: 'DM Mono, monospace',
      fontSize: size,
      fontWeight: 500,
      letterSpacing: '0.08em',
      textTransform: 'uppercase',
      color: restColor,
      lineHeight: 1,
      display: 'inline-flex',
    }}>
      <span style={{ color: sColor }}>S</span>USTENA
    </span>
  );
}

function SustenaLogo({ size = 32, gap = 12, mono = false, light = false }) {
  return (
    <div style={{ display: 'inline-flex', alignItems: 'center', gap }}>
      <SustenaMark size={size} mono={mono || light} secondary={light ? '#0f0f0f' : '#e8e4dc'} />
      <SustenaWordmark size={Math.round(size * 0.56)} mono={mono} light={light} />
    </div>
  );
}

/* ─── Artboards ──────────────────────────────────────────── */

function ArtboardLabel({ children }) {
  return (
    <div style={{
      fontFamily: 'DM Mono, monospace',
      fontSize: 10,
      letterSpacing: '0.12em',
      textTransform: 'uppercase',
      color: '#565250',
      marginBottom: 6,
    }}>{children}</div>
  );
}

function PrimaryFull() {
  return (
    <div style={{
      background: '#0f0f0f',
      width: '100%', height: '100%',
      display: 'flex', flexDirection: 'column',
      justifyContent: 'center', alignItems: 'center',
      gap: 32,
      padding: 32,
    }}>
      <SustenaLogo size={120} gap={32} />
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4 }}>
        <ArtboardLabel>PRIMARY · WORDMARK + ICON</ArtboardLabel>
        <span style={{ fontFamily: 'DM Mono, monospace', fontSize: 11, color: '#8a8680' }}>
          sustena-full-dark.svg
        </span>
      </div>
    </div>
  );
}

function IconOnly() {
  return (
    <div style={{
      background: '#0f0f0f',
      width: '100%', height: '100%',
      display: 'flex', flexDirection: 'column',
      justifyContent: 'center', alignItems: 'center',
      gap: 32,
      padding: 32,
    }}>
      <SustenaMark size={160} />
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4 }}>
        <ArtboardLabel>ICON ONLY · NETWORK NODE</ArtboardLabel>
        <span style={{ fontFamily: 'DM Mono, monospace', fontSize: 11, color: '#8a8680' }}>
          sustena-icon-only.svg
        </span>
      </div>
    </div>
  );
}

function Monochrome() {
  return (
    <div style={{
      background: '#0f0f0f',
      width: '100%', height: '100%',
      display: 'flex', flexDirection: 'column',
      justifyContent: 'center', alignItems: 'center',
      gap: 32,
      padding: 32,
    }}>
      <SustenaLogo size={120} gap={32} mono />
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4 }}>
        <ArtboardLabel>MONOCHROME · STAMPS / EMBOSS</ArtboardLabel>
        <span style={{ fontFamily: 'DM Mono, monospace', fontSize: 11, color: '#8a8680' }}>
          sustena-monochrome.svg
        </span>
      </div>
    </div>
  );
}

function LightInversion() {
  return (
    <div style={{
      background: '#e8e4dc',
      width: '100%', height: '100%',
      display: 'flex', flexDirection: 'column',
      justifyContent: 'center', alignItems: 'center',
      gap: 32,
      padding: 32,
    }}>
      <SustenaLogo size={120} gap={32} light />
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4 }}>
        <div style={{ fontFamily: 'DM Mono, monospace', fontSize: 10, letterSpacing: '0.12em', textTransform: 'uppercase', color: '#8a8680' }}>
          INVERSION · LIGHT SURFACES
        </div>
        <span style={{ fontFamily: 'DM Mono, monospace', fontSize: 11, color: '#565250' }}>
          sustena-full-light.svg
        </span>
      </div>
    </div>
  );
}

/* Construction / geometry plate */
function Construction() {
  return (
    <div style={{
      background: '#0f0f0f',
      width: '100%', height: '100%',
      display: 'flex', flexDirection: 'column',
      padding: 24,
      gap: 16,
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div>
          <ArtboardLabel>CONSTRUCTION</ArtboardLabel>
          <div style={{ fontFamily: 'Inter Tight, sans-serif', fontSize: 14, color: '#e8e4dc', marginTop: 2 }}>
            Triskele on radial 120° grid
          </div>
        </div>
        <div style={{ fontFamily: 'DM Mono, monospace', fontSize: 10, color: '#565250', textAlign: 'right', letterSpacing: '0.1em' }}>
          UNIT GRID · 200×200
        </div>
      </div>
      <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', position: 'relative' }}>
        <div style={{ position: 'relative' }}>
          <SustenaMark size={280} showGuides />
        </div>
      </div>
      <div style={{
        display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 16,
        padding: '12px 0 0', borderTop: '1px solid #2e2e2e',
      }}>
        <Spec k="OUTER R" v="78" />
        <Spec k="INNER R" v="22" />
        <Spec k="STROKE" v="14" />
        <Spec k="NODES" v="3 × 120°" />
      </div>
    </div>
  );
}

function Spec({ k, v }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
      <span style={{ fontFamily: 'DM Mono, monospace', fontSize: 10, letterSpacing: '0.1em', color: '#565250' }}>{k}</span>
      <span style={{ fontFamily: 'DM Mono, monospace', fontSize: 13, fontWeight: 500, color: '#e8e4dc' }}>{v}</span>
    </div>
  );
}

/* Scale study — favicon → masthead */
function ScaleStudy() {
  const sizes = [16, 24, 32, 48, 80, 128];
  return (
    <div style={{
      background: '#0f0f0f',
      width: '100%', height: '100%',
      padding: 24,
      display: 'flex', flexDirection: 'column', gap: 20,
    }}>
      <ArtboardLabel>SCALE STUDY · 16px → 128px</ArtboardLabel>
      <div style={{ flex: 1, display: 'flex', alignItems: 'flex-end', gap: 28, justifyContent: 'center' }}>
        {sizes.map(s => (
          <div key={s} style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 10 }}>
            <SustenaMark size={s} />
            <span style={{
              fontFamily: 'DM Mono, monospace', fontSize: 10,
              letterSpacing: '0.1em', color: '#565250',
            }}>{s}px</span>
          </div>
        ))}
      </div>
      <div style={{
        marginTop: 8, padding: '12px 0 0', borderTop: '1px solid #2e2e2e',
        fontFamily: 'Inter Tight', fontSize: 12, color: '#8a8680', lineHeight: 1.55,
      }}>
        Drop the wordmark below 24px. The mark holds geometric integrity down to 16px (favicon).
      </div>
    </div>
  );
}

/* Color tokens */
function ColorPlate() {
  const swatches = [
    { name: 'AMBER', hex: '#E8A020', use: 'CENTER NODE · ACTIVE STATE' },
    { name: 'TEXT', hex: '#e8e4dc', use: 'ARCS · WORDMARK BODY' },
    { name: 'BASE', hex: '#0f0f0f', use: 'BACKGROUND · CANONICAL' },
  ];
  return (
    <div style={{
      background: '#0f0f0f',
      width: '100%', height: '100%',
      padding: 24, display: 'flex', flexDirection: 'column', gap: 18,
    }}>
      <ArtboardLabel>COLOUR · PRIMARY MARK</ArtboardLabel>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 12, flex: 1 }}>
        {swatches.map(s => (
          <div key={s.name} style={{
            background: '#181818',
            border: '1px solid #2e2e2e',
            borderRadius: 6,
            padding: 14,
            display: 'flex', flexDirection: 'column', gap: 10,
          }}>
            <div style={{
              height: 56, borderRadius: 4, background: s.hex,
              border: s.hex === '#0f0f0f' ? '1px solid #2e2e2e' : 'none',
            }} />
            <div>
              <div style={{ fontFamily: 'DM Mono, monospace', fontSize: 11, fontWeight: 500, color: '#e8e4dc', letterSpacing: '0.08em' }}>
                {s.name}
              </div>
              <div style={{ fontFamily: 'DM Mono, monospace', fontSize: 11, color: '#8a8680', marginTop: 2 }}>
                {s.hex}
              </div>
              <div style={{ fontFamily: 'DM Mono, monospace', fontSize: 9, color: '#565250', marginTop: 6, letterSpacing: '0.1em' }}>
                {s.use}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

/* Clearspace */
function Clearspace() {
  const u = 28;
  return (
    <div style={{
      background: '#0f0f0f',
      width: '100%', height: '100%',
      padding: 24,
      display: 'flex', flexDirection: 'column', gap: 16,
    }}>
      <ArtboardLabel>CLEAR SPACE · X = HEIGHT OF MARK</ArtboardLabel>
      <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <div style={{
          position: 'relative',
          padding: u,
          border: '1px dashed #3a3a3a',
        }}>
          {/* Inner ringed clearspace */}
          <div style={{
            position: 'absolute', inset: u/2,
            border: '1px dashed #2e2e2e',
          }} />
          <SustenaLogo size={u * 2.6} gap={18} />
          {/* X labels */}
          {['top','right','bottom','left'].map((side, i) => {
            const styles = {
              top: { top: -8, left: '50%', transform: 'translateX(-50%)' },
              right: { right: -16, top: '50%', transform: 'translateY(-50%)' },
              bottom: { bottom: -8, left: '50%', transform: 'translateX(-50%)' },
              left: { left: -16, top: '50%', transform: 'translateY(-50%)' },
            }[side];
            return (
              <span key={side} style={{
                position: 'absolute', ...styles,
                background: '#0f0f0f', padding: '0 4px',
                fontFamily: 'DM Mono, monospace', fontSize: 10, color: '#565250', letterSpacing: '0.12em',
              }}>X</span>
            );
          })}
        </div>
      </div>
      <div style={{ fontFamily: 'Inter Tight', fontSize: 12, color: '#8a8680', lineHeight: 1.55, paddingTop: 12, borderTop: '1px solid #2e2e2e' }}>
        Reserve clearspace equal to mark height on all sides. Nothing competes inside the keep-out zone.
      </div>
    </div>
  );
}

/* Wordmark anatomy */
function WordmarkAnatomy() {
  return (
    <div style={{
      background: '#0f0f0f',
      width: '100%', height: '100%',
      padding: 32,
      display: 'flex', flexDirection: 'column', gap: 24,
      justifyContent: 'center',
    }}>
      <ArtboardLabel>WORDMARK · ANATOMY</ArtboardLabel>
      <div style={{ position: 'relative', display: 'flex', alignItems: 'center', justifyContent: 'center', paddingTop: 28, paddingBottom: 60 }}>
        <SustenaWordmark size={56} />
        {/* annotation lines */}
        <svg style={{ position: 'absolute', inset: 0, pointerEvents: 'none' }} width="100%" height="100%">
          {/* Indicator under the S */}
          <text x="50%" y="92%" textAnchor="middle" fontFamily="DM Mono" fontSize="10" letterSpacing="0.12em" fill="#E8A020">
            ▲ AMBER S — SIGNAL POINT
          </text>
        </svg>
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 16, paddingTop: 16, borderTop: '1px solid #2e2e2e' }}>
        <Spec k="FAMILY" v="DM MONO" />
        <Spec k="WEIGHT" v="500" />
        <Spec k="TRACKING" v="0.08em" />
        <Spec k="CASE" v="UPPER" />
      </div>
    </div>
  );
}

/* In-context: business card */
function CardContext() {
  return (
    <div style={{
      background: '#181818',
      width: '100%', height: '100%',
      padding: 24,
      display: 'flex', flexDirection: 'column', gap: 16,
    }}>
      <ArtboardLabel>IN CONTEXT · CARD</ArtboardLabel>
      <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <div style={{
          width: 360, aspectRatio: '85/55',
          background: '#0f0f0f',
          border: '1px solid #2e2e2e',
          borderRadius: 6,
          padding: 22,
          display: 'flex', flexDirection: 'column', justifyContent: 'space-between',
          position: 'relative', overflow: 'hidden',
        }}>
          <SustenaLogo size={36} gap={14} />
          <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
            <div style={{ fontFamily: 'Inter Tight', fontSize: 14, color: '#e8e4dc', fontWeight: 500 }}>
              Kwame Owusu
            </div>
            <div style={{ fontFamily: 'DM Mono, monospace', fontSize: 10, color: '#8a8680', letterSpacing: '0.1em', textTransform: 'uppercase' }}>
              COUNCIL · TREASURY OPS
            </div>
            <div style={{ fontFamily: 'DM Mono, monospace', fontSize: 10, color: '#565250', marginTop: 8 }}>
              k.owusu.eth · /node/0x4f8c…a21d
            </div>
          </div>
          {/* corner ID */}
          <div style={{
            position: 'absolute', top: 22, right: 22,
            fontFamily: 'DM Mono, monospace', fontSize: 9, color: '#565250', letterSpacing: '0.12em',
          }}>
            ID · 0148
          </div>
        </div>
      </div>
    </div>
  );
}

/* In-context: header bar */
function HeaderContext() {
  return (
    <div style={{
      background: '#181818',
      width: '100%', height: '100%',
      padding: 24,
      display: 'flex', flexDirection: 'column', gap: 16,
    }}>
      <ArtboardLabel>IN CONTEXT · APP HEADER</ArtboardLabel>
      <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <div style={{
          width: '100%', height: 44,
          background: '#181818',
          border: '1px solid #2e2e2e',
          borderRadius: 4,
          padding: '0 16px',
          display: 'flex', alignItems: 'center', gap: 16,
        }}>
          <SustenaLogo size={20} gap={8} />
          <span style={{ width: 1, height: 20, background: '#2e2e2e' }} />
          <span style={{ fontFamily: 'DM Mono, monospace', fontSize: 10, letterSpacing: '0.12em', color: '#8a8680' }}>
            TREASURY / OPERATIONS
          </span>
          <span style={{ flex: 1 }} />
          <span style={{
            padding: '3px 8px',
            border: '1px solid rgba(232,160,32,0.3)',
            background: 'rgba(232,160,32,0.12)',
            borderRadius: 4,
            fontFamily: 'DM Mono, monospace',
            fontSize: 10,
            letterSpacing: '0.12em',
            color: '#E8A020',
          }}>● PROD · LIVE</span>
        </div>
      </div>
      <div style={{ fontFamily: 'Inter Tight', fontSize: 12, color: '#8a8680', lineHeight: 1.55, paddingTop: 12, borderTop: '1px solid #2e2e2e' }}>
        Logo at 20px height in the app header, 8px gap to wordmark, separator pre-context.
      </div>
    </div>
  );
}

/* ─── Canvas composition ──────────────────────────────────── */
function LogoCanvas() {
  return (
    <DesignCanvas title="Sustena · Logo System" initialZoom={0.7}>
      <DCSection id="primary" title="Primary Mark">
        <DCArtboard id="primary-full" label="Primary · Full" width={640} height={400}>
          <PrimaryFull />
        </DCArtboard>
        <DCArtboard id="icon-only" label="Icon Only" width={460} height={400}>
          <IconOnly />
        </DCArtboard>
        <DCArtboard id="monochrome" label="Monochrome" width={640} height={400}>
          <Monochrome />
        </DCArtboard>
        <DCArtboard id="light-inv" label="Light Inversion" width={640} height={400}>
          <LightInversion />
        </DCArtboard>
      </DCSection>

      <DCSection id="anatomy" title="Anatomy & Construction">
        <DCArtboard id="construction" label="Construction" width={520} height={520}>
          <Construction />
        </DCArtboard>
        <DCArtboard id="wordmark" label="Wordmark Anatomy" width={520} height={300}>
          <WordmarkAnatomy />
        </DCArtboard>
        <DCArtboard id="scale" label="Scale Study" width={760} height={300}>
          <ScaleStudy />
        </DCArtboard>
        <DCArtboard id="clear" label="Clearspace" width={520} height={420}>
          <Clearspace />
        </DCArtboard>
        <DCArtboard id="color" label="Colour" width={760} height={300}>
          <ColorPlate />
        </DCArtboard>
      </DCSection>

      <DCSection id="context" title="In Context">
        <DCArtboard id="header" label="App Header" width={760} height={280}>
          <HeaderContext />
        </DCArtboard>
        <DCArtboard id="card" label="Identity Card" width={520} height={400}>
          <CardContext />
        </DCArtboard>
      </DCSection>
    </DesignCanvas>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<LogoCanvas />);

/* Export Mark so the dashboard can re-use it */
window.SustenaMark = SustenaMark;
window.SustenaWordmark = SustenaWordmark;
window.SustenaLogoFull = SustenaLogo;
