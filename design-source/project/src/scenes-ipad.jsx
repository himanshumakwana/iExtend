// iPad scenes for iExtend
// Each scene is a self-contained React component rendered inside <IPad>.
// They take props { dark, density, toolbarPos, conn } from the parent state.

const { useState: useStateI, useEffect: useEffectI } = React;

// ─────────────────────────────────────────────────────────
// Theme tokens for iPadOS-ish look
// ─────────────────────────────────────────────────────────
const ipadTheme = (dark) => ({
  bg: dark ? '#000' : '#f2f2f7',
  card: dark ? '#1c1c1e' : '#ffffff',
  card2: dark ? '#2c2c2e' : '#ffffff',
  ink: dark ? '#ffffff' : '#000000',
  ink2: dark ? 'rgba(235,235,245,0.6)' : 'rgba(60,60,67,0.6)',
  ink3: dark ? 'rgba(235,235,245,0.3)' : 'rgba(60,60,67,0.3)',
  sep: dark ? 'rgba(84,84,88,0.5)' : 'rgba(60,60,67,0.18)',
  field: dark ? '#1c1c1e' : '#ffffff',
  groupBg: dark ? '#000' : '#f2f2f7',
  blue: '#0a84ff',
  indigo: '#5e5ce6',
  green: '#30d158',
  red: '#ff453a',
  orange: '#ff9f0a',
});

// Glass blob that sits as background in onboarding scenes
function Aurora({ dark }) {
  return (
    <div style={{
      position: 'absolute', inset: 0, zIndex: 0,
      background: dark
        ? `radial-gradient(700px 380px at 25% 15%, rgba(94,92,230,0.45), transparent 60%),
           radial-gradient(700px 420px at 80% 90%, rgba(10,132,255,0.40), transparent 60%),
           radial-gradient(500px 320px at 90% 10%, rgba(255,159,10,0.18), transparent 60%)`
        : `radial-gradient(700px 380px at 25% 15%, rgba(94,92,230,0.20), transparent 60%),
           radial-gradient(700px 420px at 80% 90%, rgba(10,132,255,0.20), transparent 60%),
           radial-gradient(500px 320px at 90% 10%, rgba(255,159,10,0.10), transparent 60%)`,
    }}/>
  );
}

// CTA pill button
function PillBtn({ children, onClick, primary = false, dark = false, ghost = false, full = false, dim = false, style = {} }) {
  const t = ipadTheme(dark);
  let bg, color, border = 'none';
  if (primary) { bg = t.blue; color = '#fff'; }
  else if (ghost) { bg = 'transparent'; color = t.blue; }
  else { bg = dark ? 'rgba(120,120,128,0.32)' : 'rgba(120,120,128,0.16)'; color = t.ink; }
  if (dim) bg = dark ? 'rgba(120,120,128,0.14)' : 'rgba(120,120,128,0.08)';
  return (
    <button onClick={onClick} style={{
      appearance: 'none', border, background: bg, color,
      padding: '12px 22px', borderRadius: 12,
      fontFamily: '-apple-system, "SF Pro Text", system-ui',
      fontSize: 15, fontWeight: 600, letterSpacing: -0.2,
      cursor: 'pointer', width: full ? '100%' : undefined, ...style,
    }}>
      {children}
    </button>
  );
}

// =============================================================
// 1) ONBOARDING — welcome
// =============================================================
function SceneWelcome({ dark }) {
  const t = ipadTheme(dark);
  return (
    <div style={{ position: 'absolute', inset: 0, background: t.bg }}>
      <Aurora dark={dark}/>
      <IPadStatusBar dark={dark}/>
      <div style={{
        position: 'relative', zIndex: 2,
        height: '100%',
        display: 'grid', gridTemplateColumns: '1.05fr 1fr',
      }}>
        {/* Left — copy */}
        <div style={{ display: 'flex', flexDirection: 'column', justifyContent: 'center', padding: '40px 8px 40px 56px' }}>
          <div style={{
            display: 'inline-flex', alignSelf: 'flex-start', alignItems: 'center', gap: 8,
            padding: '6px 12px', borderRadius: 999,
            background: dark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.04)',
            border: `1px solid ${t.sep}`, color: t.ink2, fontSize: 12, fontWeight: 500,
          }}>
            <span style={{ width: 6, height: 6, borderRadius: 99, background: t.blue }}/>
            iExtend 1.0
          </div>
          <h1 className="sf-display" style={{
            fontSize: 44, lineHeight: 1.05, fontWeight: 700, letterSpacing: -0.04,
            color: t.ink, margin: '14px 0 12px',
          }}>
            Your iPad,<br/>
            <span style={{
              background: 'linear-gradient(120deg, #0a84ff, #5e5ce6 60%, #ff9f0a)',
              WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent',
            }}>a second screen.</span>
          </h1>
          <p style={{
            fontSize: 15, lineHeight: 1.5, color: t.ink2, margin: '0 0 22px', maxWidth: 320,
          }}>
            Extend your PC to your iPad over Wi‑Fi. No cables, no drivers — just pick a workspace and keep going.
          </p>
          <div style={{ display: 'flex', gap: 10 }}>
            <PillBtn primary>Get started</PillBtn>
            <PillBtn dark={dark} ghost>Learn more</PillBtn>
          </div>
          <div style={{ marginTop: 28, display: 'flex', gap: 22, color: t.ink2, fontSize: 13 }}>
            {[['Wi‑Fi or USB', I.wifi(14, t.ink2)], ['Pencil ready', I.pencil(14, t.ink2)], ['Up to 120 Hz', I.bolt(14, t.ink2)]].map(([k, ic], i) => (
              <span key={i} style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>{ic}<span>{k}</span></span>
            ))}
          </div>
        </div>
        {/* Right — illustration of mode picker */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', padding: '0 30px 0 0' }}>
          <div style={{
            width: 270, height: 340, borderRadius: 28,
            background: dark ? 'rgba(28,28,30,0.7)' : 'rgba(255,255,255,0.65)',
            backdropFilter: 'blur(24px) saturate(160%)',
            WebkitBackdropFilter: 'blur(24px) saturate(160%)',
            border: `1px solid ${dark ? 'rgba(255,255,255,0.1)' : 'rgba(0,0,0,0.05)'}`,
            boxShadow: dark ? '0 30px 60px rgba(0,0,0,0.5)' : '0 30px 60px rgba(0,0,0,0.12)',
            padding: 22, display: 'flex', flexDirection: 'column', gap: 12,
          }}>
            <div style={{ fontSize: 13, color: t.ink2, fontWeight: 600, letterSpacing: -0.1 }}>How will you use it?</div>
            {[
              { t: 'Extend desktop', s: 'More room for windows', icon: I.extend(28, t.blue), tint: 'rgba(10,132,255,0.12)', selected: true },
              { t: 'Mirror screen', s: 'Show the same view', icon: I.mirror(28, t.indigo), tint: 'rgba(94,92,230,0.12)' },
              { t: 'Drawing tablet', s: 'Pencil + Wacom mode', icon: I.pencil(28, t.orange), tint: 'rgba(255,159,10,0.14)' },
            ].map((m, i) => (
              <div key={i} style={{
                display: 'flex', alignItems: 'center', gap: 14,
                padding: '14px 16px', borderRadius: 16,
                background: m.selected ? m.tint : (dark ? 'rgba(255,255,255,0.04)' : 'rgba(0,0,0,0.03)'),
                border: m.selected ? `1.5px solid ${t.blue}` : `1px solid ${t.sep}`,
              }}>
                <div style={{ width: 44, height: 44, borderRadius: 12, background: m.tint, display: 'grid', placeItems: 'center' }}>{m.icon}</div>
                <div style={{ flex: 1 }}>
                  <div style={{ fontSize: 15, fontWeight: 600, color: t.ink }}>{m.t}</div>
                  <div style={{ fontSize: 12, color: t.ink2 }}>{m.s}</div>
                </div>
                {m.selected && <span style={{ color: t.blue }}>{I.check(16, t.blue)}</span>}
              </div>
            ))}
          </div>
        </div>
      </div>
      <IPadHomeIndicator dark={dark}/>
    </div>
  );
}

// =============================================================
// 2) DISCOVERY — devices on Wi-Fi
// =============================================================
function SceneDiscovery({ dark }) {
  const t = ipadTheme(dark);
  const devices = [
    { name: "Aman's PC",       sub: 'Windows 11 · 192.168.1.42', signal: 4, ms: 6,  selected: true },
    { name: 'Studio Tower',     sub: 'Windows 11 · 192.168.1.15', signal: 4, ms: 11 },
    { name: 'MacBook Pro',      sub: 'macOS · 192.168.1.27',      signal: 3, ms: 18 },
    { name: 'Linux Workstation',sub: 'Ubuntu · 192.168.1.51',     signal: 2, ms: 24 },
  ];
  return (
    <div style={{ position: 'absolute', inset: 0, background: t.bg }}>
      <Aurora dark={dark}/>
      <IPadStatusBar dark={dark}/>
      <div style={{ position: 'relative', zIndex: 2, height: '100%', display: 'flex', flexDirection: 'column', padding: '38px 36px 22px' }}>
        <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between' }}>
          <div>
            <div style={{ fontSize: 11, color: t.ink2, fontWeight: 500, letterSpacing: 0.1, textTransform: 'uppercase' }}>Step 2 of 3 · Discover</div>
            <h2 className="sf-display" style={{ fontSize: 28, fontWeight: 700, letterSpacing: -0.03, color: t.ink, margin: '4px 0 4px' }}>
              Looking for your computer…
            </h2>
            <div style={{ fontSize: 13, color: t.ink2, display: 'inline-flex', alignItems: 'center', gap: 6 }}>
              {I.wifi(13, t.ink2)} On <b style={{ color: t.ink }}>HomeNet 5G</b> · 4 devices
            </div>
          </div>
          <div style={{ display: 'flex', gap: 6 }}>
            <PillBtn dark={dark} dim style={{ padding: '7px 11px', fontSize: 13, display: 'inline-flex', alignItems: 'center', gap: 6 }}>
              {I.refresh(13, t.ink)} Rescan
            </PillBtn>
            <PillBtn dark={dark} dim style={{ padding: '7px 11px', fontSize: 13, display: 'inline-flex', alignItems: 'center', gap: 6 }}>
              {I.plus(13, t.ink)} Manual IP
            </PillBtn>
          </div>
        </div>

        {/* Device list */}
        <div style={{
          marginTop: 14, borderRadius: 18, overflow: 'hidden',
          background: t.card, border: `1px solid ${t.sep}`,
        }}>
          {devices.map((d, i) => (
            <div key={i} style={{
              display: 'flex', alignItems: 'center', gap: 12, padding: '11px 16px',
              borderTop: i ? `0.5px solid ${t.sep}` : 'none',
              background: d.selected ? (dark ? 'rgba(10,132,255,0.14)' : 'rgba(10,132,255,0.08)') : 'transparent',
            }}>
              <div style={{
                width: 36, height: 36, borderRadius: 10,
                background: dark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.04)',
                display: 'grid', placeItems: 'center', color: d.selected ? t.blue : t.ink,
              }}>
                {I.monitor(18, d.selected ? t.blue : t.ink)}
              </div>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 14, fontWeight: 600, color: t.ink, letterSpacing: -0.2 }}>{d.name}</div>
                <div style={{ fontSize: 11, color: t.ink2, marginTop: 1 }}>{d.sub}</div>
              </div>
              {/* signal bars */}
              <div style={{ display: 'flex', alignItems: 'flex-end', gap: 2, marginRight: 8 }}>
                {[1,2,3,4].map(b => (
                  <div key={b} style={{
                    width: 3, height: 3 + b*2.5, borderRadius: 1,
                    background: b <= d.signal ? t.ink : t.ink3,
                  }}/>
                ))}
              </div>
              <div style={{ fontSize: 11, color: t.ink2, fontVariantNumeric: 'tabular-nums', minWidth: 50, textAlign: 'right' }}>
                ~{d.ms} ms
              </div>
              {d.selected
                ? <PillBtn primary style={{ padding: '6px 12px', fontSize: 12, marginLeft: 10 }}>Connect</PillBtn>
                : <span style={{ color: t.ink3, marginLeft: 10 }}>{I.chevR(13, t.ink3)}</span>}
            </div>
          ))}
        </div>

        <div style={{ marginTop: 'auto', paddingTop: 12, display: 'flex', alignItems: 'center', gap: 8, color: t.ink2, fontSize: 11 }}>
          <span style={{
            width: 22, height: 22, borderRadius: 6,
            background: dark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.04)',
            display: 'grid', placeItems: 'center', color: t.blue,
          }}>{I.bolt(12, t.blue)}</span>
          Don't see your PC? Make sure the iExtend desktop app is running on the same network.
        </div>
      </div>
      <IPadHomeIndicator dark={dark}/>
    </div>
  );
}

// =============================================================
// 3) PAIRING — QR + PIN
// =============================================================
function ScenePairing({ dark }) {
  const t = ipadTheme(dark);
  const pin = ['4','7','2','9'];
  return (
    <div style={{ position: 'absolute', inset: 0, background: t.bg }}>
      <Aurora dark={dark}/>
      <IPadStatusBar dark={dark}/>
      <div style={{ position: 'relative', zIndex: 2, height: '100%', display: 'grid', gridTemplateColumns: '1fr 0.95fr', padding: '40px 36px 28px', gap: 22 }}>
        {/* Left — QR card */}
        <div style={{
          background: t.card, borderRadius: 24, border: `1px solid ${t.sep}`,
          padding: 20, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
        }}>
          <div style={{ fontSize: 11, color: t.ink2, alignSelf: 'flex-start', textTransform: 'uppercase', letterSpacing: 0.1, fontWeight: 600 }}>
            Scan from your PC
          </div>
          <div style={{ fontSize: 17, fontWeight: 700, color: t.ink, alignSelf: 'flex-start', margin: '4px 0 14px', letterSpacing: -0.02, lineHeight: 1.2 }}>
            Point the iExtend app's camera at this code
          </div>
          {/* faux QR */}
          <div style={{
            width: 180, height: 180, padding: 12, borderRadius: 18, background: '#fff',
            boxShadow: '0 14px 38px rgba(0,0,0,0.18)',
            display: 'grid', gridTemplateColumns: 'repeat(21, 1fr)', gap: 1,
          }}>
            {Array.from({ length: 21*21 }).map((_, i) => {
              const r = (i * 9301 + 49297) % 233280 / 233280;
              // corner finder squares
              const x = i % 21, y = Math.floor(i / 21);
              const inFinder = (xi, yi) =>
                (x >= xi && x <= xi+6 && y >= yi && y <= yi+6) &&
                !(x > xi+1 && x < xi+5 && y > yi+1 && y < yi+5) ||
                (x >= xi+2 && x <= xi+4 && y >= yi+2 && y <= yi+4);
              const finder = inFinder(0,0) || inFinder(14,0) || inFinder(0,14);
              return <div key={i} style={{ background: finder || r > 0.5 ? '#000' : '#fff' }}/>;
            })}
          </div>
          <div style={{
            marginTop: 18, fontFamily: 'ui-monospace, "SF Mono", monospace',
            fontSize: 13, color: t.ink2,
          }}>iextend://pair/9k2-4npx</div>
        </div>

        {/* Right — PIN entry */}
        <div style={{
          background: t.card, borderRadius: 22, border: `1px solid ${t.sep}`,
          padding: 18, display: 'flex', flexDirection: 'column',
        }}>
          <div style={{ fontSize: 13, color: t.ink2, textTransform: 'uppercase', letterSpacing: 0.1, fontWeight: 600 }}>
            Or enter PIN from PC
          </div>
          <div style={{ fontSize: 16, fontWeight: 700, color: t.ink, margin: '4px 0 14px', letterSpacing: -0.02, lineHeight: 1.2 }}>
            Aman's PC shows a 4‑digit code
          </div>
          {/* Pin boxes */}
          <div style={{ display: 'flex', gap: 8, justifyContent: 'center', marginBottom: 12 }}>
            {pin.map((d, i) => (
              <div key={i} style={{
                width: 42, height: 52, borderRadius: 10,
                background: dark ? 'rgba(255,255,255,0.04)' : 'rgba(0,0,0,0.04)',
                border: `1.5px solid ${i === 3 ? t.blue : t.sep}`,
                display: 'grid', placeItems: 'center',
                fontFamily: '-apple-system, "SF Pro Display"', fontSize: 22, fontWeight: 600,
                color: t.ink, letterSpacing: -0.02,
              }}>{d}</div>
            ))}
          </div>
          {/* Numeric pad */}
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 6, marginTop: 'auto' }}>
            {['1','2','3','4','5','6','7','8','9','','0','⌫'].map((k, i) => (
              <div key={i} style={{
                height: 28, borderRadius: 8,
                background: k === '' ? 'transparent' : (dark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.04)'),
                display: 'grid', placeItems: 'center',
                fontFamily: '-apple-system, "SF Pro Display"', fontSize: 14, fontWeight: 500,
                color: t.ink,
                visibility: k === '' ? 'hidden' : undefined,
              }}>{k}</div>
            ))}
          </div>
          <div style={{ marginTop: 4, fontSize: 11, color: t.ink2, textAlign: 'center' }}>
            Pairing expires in <b style={{ color: t.ink }}>0:54</b>
          </div>
        </div>
      </div>
      <IPadHomeIndicator dark={dark}/>
    </div>
  );
}

// =============================================================
// 4) LIVE — connected (extended desktop / mirror)
// =============================================================
function SceneLive({ dark, conn = 'live', toolbarPos = 'bottom', density = 'regular' }) {
  const t = ipadTheme(dark);

  // Fake "extended desktop" content — the iPad shows the right half of the user's PC desktop
  const desktopBg = (
    <div style={{
      position: 'absolute', inset: 0,
      background: `
        radial-gradient(900px 500px at 30% 20%, rgba(94,92,230,0.55), transparent 60%),
        radial-gradient(700px 500px at 80% 80%, rgba(10,132,255,0.45), transparent 60%),
        linear-gradient(180deg, #0c1430 0%, #131736 60%, #0a0d22 100%)`,
    }}/>
  );

  return (
    <div style={{ position: 'absolute', inset: 0, background: '#000', overflow: 'hidden' }}>
      {/* Live "screen" content */}
      {desktopBg}
      {/* Some windows on the extended desktop */}
      <FakeWindow
        x={30} y={48} w={320} h={210} title="figma — iExtend.fig"
        accent="#5e5ce6"
        body={<FigmaCanvas/>}
      />
      <FakeWindow
        x={290} y={250} w={360} h={220} title="Spotify — focus.flow"
        accent="#1db954"
        body={<MusicPlayer/>}
      />
      <FakeWindow
        x={70} y={290} w={240} h={170} title="Terminal"
        accent="#0a84ff" dark
        body={
          <div className="mono" style={{ padding: 12, color: '#9bd', fontSize: 11, lineHeight: 1.5 }}>
            <div><span style={{ color: '#5e5ce6' }}>~/iextend</span> $ npm run dev</div>
            <div style={{ color: '#5fc' }}>▲ ready in 412ms</div>
            <div style={{ color: '#aab' }}>→ Local:   http://localhost:3000</div>
            <div style={{ color: '#aab' }}>→ Network: 192.168.1.42:3000</div>
            <div><span style={{ color: '#5e5ce6' }}>~/iextend</span> $ <span style={{ background: '#9bd', color: '#000', padding: '0 2px', animation: 'blink 1s infinite' }}> </span></div>
          </div>
        }
      />

      <IPadStatusBar dark latencyMs={conn === 'live' ? 8 : undefined}/>

      {/* Floating toolbar */}
      {conn === 'live' && (
        <FloatingToolbar dark={dark} pos={toolbarPos} density={density} t={t}/>
      )}

      {/* Connecting overlay */}
      {conn === 'connecting' && <ConnectingOverlay dark/>}
      {/* Disconnected overlay */}
      {conn === 'error' && <DisconnectedOverlay/>}

      <IPadHomeIndicator dark/>
    </div>
  );
}

// faux window on the extended desktop
function FakeWindow({ x, y, w, h, title, accent = '#0a84ff', dark = false, body }) {
  return (
    <div style={{
      position: 'absolute', left: x, top: y, width: w, height: h,
      background: dark ? '#1a1c20' : '#fff',
      borderRadius: 10, overflow: 'hidden',
      boxShadow: '0 30px 60px rgba(0,0,0,0.45), 0 0 0 1px rgba(255,255,255,0.06)',
      color: dark ? '#fff' : '#1a1a1a',
    }}>
      <div style={{
        height: 26, display: 'flex', alignItems: 'center', gap: 7, padding: '0 10px',
        background: dark ? '#15171b' : '#f3f3f3',
        borderBottom: `1px solid ${dark ? '#262830' : 'rgba(0,0,0,0.06)'}`,
      }}>
        {['#ff5f57','#febc2e','#28c840'].map((c, i) => (
          <span key={i} style={{ width: 10, height: 10, borderRadius: 99, background: c }}/>
        ))}
        <span style={{ marginLeft: 8, fontSize: 11, fontFamily: '"Segoe UI", system-ui', opacity: 0.8 }}>{title}</span>
      </div>
      <div style={{ position: 'absolute', inset: '26px 0 0 0' }}>{body}</div>
    </div>
  );
}

function FigmaCanvas() {
  return (
    <div style={{ position: 'absolute', inset: 0, background: '#1e1e1e' }}>
      {/* sidebar */}
      <div style={{ position: 'absolute', left: 0, top: 0, bottom: 0, width: 50, background: '#2c2c2c', borderRight: '1px solid #1a1a1a' }}/>
      {/* canvas */}
      <div style={{ position: 'absolute', inset: '0 0 0 50px', background: '#1e1e1e', display: 'grid', placeItems: 'center' }}>
        <div style={{
          width: 140, height: 100, borderRadius: 8, background: 'linear-gradient(140deg,#0a84ff,#5e5ce6)',
          boxShadow: '0 4px 14px rgba(10,132,255,0.4), 0 0 0 1px #0a84ff',
        }}/>
      </div>
      {/* right panel */}
      <div style={{ position: 'absolute', right: 0, top: 0, bottom: 0, width: 70, background: '#2c2c2c', borderLeft: '1px solid #1a1a1a',
        display: 'flex', flexDirection: 'column', gap: 4, padding: 6 }}>
        {[1,2,3,4].map(i => <div key={i} style={{ height: 18, background: '#383838', borderRadius: 3 }}/>)}
      </div>
    </div>
  );
}

function MusicPlayer() {
  return (
    <div style={{ position: 'absolute', inset: 0, background: '#121212', color: '#fff', padding: 14 }}>
      <div style={{ display: 'flex', gap: 12 }}>
        <div style={{ width: 70, height: 70, borderRadius: 6, background: 'linear-gradient(140deg,#1db954,#073)' }}/>
        <div>
          <div style={{ fontSize: 12, color: '#1db954', fontWeight: 600 }}>NOW PLAYING</div>
          <div style={{ fontSize: 14, fontWeight: 700, marginTop: 4 }}>focus.flow</div>
          <div style={{ fontSize: 11, color: '#9a9a9a' }}>deep work, vol. 3</div>
        </div>
      </div>
      <div style={{ marginTop: 18, height: 3, borderRadius: 99, background: '#333', position: 'relative' }}>
        <div style={{ position: 'absolute', left: 0, top: 0, bottom: 0, width: '38%', background: '#1db954', borderRadius: 99 }}/>
      </div>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, color: '#9a9a9a', marginTop: 4 }}>
        <span>1:42</span><span>4:31</span>
      </div>
    </div>
  );
}

// Floating toolbar — pos: top | bottom | left
function FloatingToolbar({ dark, pos, density, t }) {
  const compact = density === 'compact';
  const comfy = density === 'comfy';
  const pad = compact ? '6px 8px' : comfy ? '12px 16px' : '9px 12px';
  const gap = compact ? 4 : comfy ? 12 : 8;
  const btn = compact ? 32 : comfy ? 44 : 38;

  const style = (() => {
    const horizontal = { left: '50%', transform: 'translateX(-50%)' };
    if (pos === 'top') return { ...horizontal, top: 38 };
    if (pos === 'left') return { left: 16, top: '50%', transform: 'translateY(-50%)' };
    return { ...horizontal, bottom: 28 }; // bottom
  })();
  const vertical = pos === 'left';

  const items = [
    { key: 'mode', icon: I.extend(20, '#fff'), label: 'Mode' },
    { key: 'res',  icon: I.monitor(20, '#fff'), label: 'Res' },
    { key: 'lat',  icon: <span style={{ width: 8, height: 8, borderRadius: 99, background: '#30d158' }}/>, label: '8 ms' },
    { key: 'pen',  icon: I.pencil(20, '#fff'), label: 'Pencil' },
    { key: 'hand', icon: I.hand(20, '#fff'), label: 'Pan' },
    { key: 'gear', icon: I.gear(20, '#fff'), label: 'Settings' },
    { key: 'end',  icon: I.power(18, '#ff453a'), label: 'End', danger: true },
  ];

  return (
    <div style={{
      position: 'absolute', zIndex: 40, ...style,
      display: 'flex', flexDirection: vertical ? 'column' : 'row', alignItems: 'center',
      gap, padding: pad,
      background: 'rgba(20,20,22,0.6)',
      border: '1px solid rgba(255,255,255,0.12)',
      borderRadius: 999,
      backdropFilter: 'blur(24px) saturate(180%)',
      WebkitBackdropFilter: 'blur(24px) saturate(180%)',
      boxShadow: '0 14px 40px rgba(0,0,0,0.45), inset 0 1px 0 rgba(255,255,255,0.12)',
      color: '#fff',
    }}>
      {/* drag handle */}
      <div style={{
        width: vertical ? 22 : 4, height: vertical ? 4 : 22, borderRadius: 99,
        background: 'rgba(255,255,255,0.25)', margin: vertical ? '4px 0' : '0 4px 0 0', flexShrink: 0,
      }}/>
      {items.map((it, i) => (
        <div key={it.key} style={{
          width: btn, height: btn, borderRadius: 999,
          background: it.danger ? 'rgba(255,69,58,0.16)' : 'rgba(255,255,255,0.08)',
          border: '1px solid rgba(255,255,255,0.08)',
          display: 'grid', placeItems: 'center',
          color: it.danger ? '#ff453a' : '#fff',
          position: 'relative',
        }} title={it.label}>
          {it.icon}
          {it.key === 'lat' && !compact && (
            <span style={{ position: 'absolute', left: '50%', bottom: -16, transform: 'translateX(-50%)', fontSize: 9, color: '#30d158', whiteSpace: 'nowrap' }}>{it.label}</span>
          )}
        </div>
      ))}
    </div>
  );
}

function ConnectingOverlay({ dark }) {
  return (
    <div style={{
      position: 'absolute', inset: 0, zIndex: 50,
      background: 'rgba(0,0,0,0.55)', backdropFilter: 'blur(12px)',
      display: 'grid', placeItems: 'center',
    }}>
      <div style={{
        width: 320, padding: 28, borderRadius: 24, textAlign: 'center',
        background: 'rgba(28,28,30,0.85)', color: '#fff',
        border: '1px solid rgba(255,255,255,0.1)',
        boxShadow: '0 30px 60px rgba(0,0,0,0.5)',
      }}>
        <Spinner/>
        <div style={{ fontSize: 17, fontWeight: 600, marginTop: 14 }}>Connecting to Aman's PC…</div>
        <div style={{ fontSize: 13, color: 'rgba(255,255,255,0.6)', marginTop: 4 }}>Negotiating display · 1 of 3</div>
        <div style={{ marginTop: 16, height: 4, borderRadius: 99, background: 'rgba(255,255,255,0.1)', overflow: 'hidden' }}>
          <div style={{ width: '40%', height: '100%', background: 'linear-gradient(90deg,#0a84ff,#5e5ce6)' }}/>
        </div>
      </div>
    </div>
  );
}

function DisconnectedOverlay() {
  return (
    <div style={{
      position: 'absolute', inset: 0, zIndex: 50,
      background: 'rgba(0,0,0,0.65)', backdropFilter: 'blur(8px)',
      display: 'grid', placeItems: 'center',
    }}>
      <div style={{
        width: 360, padding: 28, borderRadius: 24, textAlign: 'center',
        background: 'rgba(28,28,30,0.92)', color: '#fff',
        border: '1px solid rgba(255,69,58,0.4)',
        boxShadow: '0 30px 60px rgba(0,0,0,0.55)',
      }}>
        <div style={{ width: 56, height: 56, margin: '0 auto', borderRadius: 999, background: 'rgba(255,69,58,0.18)', display: 'grid', placeItems: 'center' }}>
          {I.warn(28, '#ff453a')}
        </div>
        <div style={{ fontSize: 19, fontWeight: 700, marginTop: 14 }}>Lost connection to PC</div>
        <div style={{ fontSize: 13, color: 'rgba(255,255,255,0.65)', marginTop: 6, lineHeight: 1.45 }}>
          Wi‑Fi signal dropped at <span style={{ fontFamily: 'ui-monospace,monospace' }}>192.168.1.42</span>.<br/>
          Reconnecting in <b style={{ color: '#fff' }}>3s…</b>
        </div>
        <div style={{ display: 'flex', gap: 10, marginTop: 18 }}>
          <PillBtn primary full>Try again now</PillBtn>
          <PillBtn dark full>Cancel</PillBtn>
        </div>
      </div>
    </div>
  );
}

function Spinner() {
  return (
    <div style={{ width: 38, height: 38, margin: '0 auto', position: 'relative' }}>
      <style>{`@keyframes spin{to{transform:rotate(360deg)}} @keyframes blink{50%{opacity:0}}`}</style>
      <svg width="38" height="38" viewBox="0 0 38 38" style={{ animation: 'spin 1s linear infinite' }}>
        <circle cx="19" cy="19" r="15" stroke="rgba(255,255,255,0.15)" strokeWidth="3" fill="none"/>
        <path d="M19 4 a15 15 0 0 1 15 15" stroke="#0a84ff" strokeWidth="3" strokeLinecap="round" fill="none"/>
      </svg>
    </div>
  );
}

// =============================================================
// 5) SETTINGS — list-style iPadOS
// =============================================================
function SceneSettings({ dark }) {
  const t = ipadTheme(dark);
  return (
    <div style={{ position: 'absolute', inset: 0, background: t.groupBg }}>
      <IPadStatusBar dark={dark}/>
      <div style={{ position: 'absolute', inset: '34px 0 24px 0', display: 'grid', gridTemplateColumns: '300px 1fr' }}>
        {/* Sidebar */}
        <div style={{ borderRight: `0.5px solid ${t.sep}`, padding: '14px 0' }}>
          <div style={{ padding: '8px 22px', fontSize: 28, fontWeight: 700, color: t.ink, letterSpacing: -0.02 }}>Settings</div>
          <div style={{ padding: '0 12px' }}>
            <SidebarSearch dark={dark}/>
          </div>
          <SidebarSection dark={dark} items={[
            { i: I.link(18, t.blue), label: 'Connection', tint: t.blue, selected: true, badge: 'Aman\'s PC' },
            { i: I.monitor(18, t.indigo), label: 'Display', tint: t.indigo },
            { i: I.pencil(18, t.orange), label: 'Pencil & Touch', tint: t.orange },
            { i: I.bolt(18, t.green), label: 'Performance', tint: t.green },
          ]}/>
          <SidebarSection dark={dark} items={[
            { i: I.gear(18, t.ink2), label: 'General', tint: 'rgba(0,0,0,0.1)' },
            { i: I.warn(18, t.red), label: 'Diagnostics', tint: t.red },
          ]}/>
        </div>
        {/* Detail pane */}
        <div style={{ padding: '14px 28px 24px', overflow: 'hidden' }}>
          <div style={{
            fontSize: 28, fontWeight: 700, color: t.ink, letterSpacing: -0.02, padding: '8px 14px 12px',
          }}>Connection</div>

          <ListGroup dark={dark} header="Active session">
            <Row dark={dark} title="Connected to" detail="Aman's PC"/>
            <Row dark={dark} title="Network" detail="HomeNet 5G · Wi‑Fi 6"/>
            <Row dark={dark} title="Mode" detail="Extended desktop" chevron/>
            <Row dark={dark} title="Latency" right={<LatencySpark/>} last/>
          </ListGroup>

          <ListGroup dark={dark} header="Connection method">
            <SegmentRow dark={dark} options={['Wi‑Fi', 'USB‑C', 'Auto']} selected={0}/>
            <Row dark={dark} title="Auto‑connect on launch" right={<Toggle on/>} last/>
          </ListGroup>

          <ListGroup dark={dark} header="Display">
            <Row dark={dark} title="Resolution" detail="1920 × 1200 @ 120 Hz" chevron/>
            <Row dark={dark} title="Scaling" right={<Slider value={0.6}/>} />
            <Row dark={dark} title="Color" detail="P3 Wide" chevron/>
            <Row dark={dark} title="HDR" right={<Toggle/>} last/>
          </ListGroup>
        </div>
      </div>
      <IPadHomeIndicator dark={dark}/>
    </div>
  );
}

function SidebarSearch({ dark }) {
  const t = ipadTheme(dark);
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 8,
      background: dark ? 'rgba(120,120,128,0.24)' : 'rgba(118,118,128,0.12)',
      padding: '7px 10px', borderRadius: 10, color: t.ink2, fontSize: 14,
    }}>
      {I.search(15, t.ink2)} <span style={{ flex: 1 }}>Search</span>
    </div>
  );
}
function SidebarSection({ items, dark }) {
  const t = ipadTheme(dark);
  return (
    <div style={{ padding: '14px 8px 0' }}>
      {items.map((it, i) => (
        <div key={i} style={{
          display: 'flex', alignItems: 'center', gap: 12,
          padding: '8px 12px', borderRadius: 10, marginBottom: 2,
          background: it.selected ? (dark ? 'rgba(120,120,128,0.36)' : 'rgba(120,120,128,0.18)') : 'transparent',
          color: t.ink, fontSize: 15, fontWeight: 500,
        }}>
          <span style={{
            width: 28, height: 28, borderRadius: 7,
            background: it.tint, display: 'grid', placeItems: 'center',
          }}>{it.i}</span>
          <span style={{ flex: 1 }}>{it.label}</span>
          {it.badge && <span style={{ fontSize: 12, color: t.ink2 }}>{it.badge}</span>}
          <span style={{ color: t.ink3 }}>{I.chevR(11, t.ink3)}</span>
        </div>
      ))}
    </div>
  );
}
function ListGroup({ header, children, dark }) {
  const t = ipadTheme(dark);
  return (
    <div style={{ marginTop: 18 }}>
      {header && <div style={{ fontSize: 12, color: t.ink2, padding: '0 30px 6px', textTransform: 'uppercase', letterSpacing: 0.06 }}>{header}</div>}
      <div style={{ background: t.card, borderRadius: 18, margin: '0 14px', overflow: 'hidden', border: `0.5px solid ${t.sep}` }}>
        {children}
      </div>
    </div>
  );
}
function Row({ title, detail, right, chevron = false, last = false, dark = false }) {
  const t = ipadTheme(dark);
  return (
    <div style={{
      display: 'flex', alignItems: 'center', minHeight: 44, padding: '8px 16px',
      borderBottom: last ? 'none' : `0.5px solid ${t.sep}`,
      fontSize: 15, color: t.ink,
    }}>
      <span style={{ flex: 1 }}>{title}</span>
      {right ? right : detail && <span style={{ color: t.ink2 }}>{detail}</span>}
      {chevron && <span style={{ color: t.ink3, marginLeft: 8 }}>{I.chevR(11, t.ink3)}</span>}
    </div>
  );
}
function Toggle({ on = false }) {
  return (
    <div style={{
      width: 51, height: 31, borderRadius: 99,
      background: on ? '#30d158' : 'rgba(120,120,128,0.32)',
      position: 'relative', transition: 'background .2s',
    }}>
      <div style={{
        position: 'absolute', top: 2, left: on ? 22 : 2,
        width: 27, height: 27, borderRadius: 99, background: '#fff',
        boxShadow: '0 2px 6px rgba(0,0,0,0.18), 0 0 0 0.5px rgba(0,0,0,0.04)',
        transition: 'left .2s',
      }}/>
    </div>
  );
}
function Slider({ value = 0.5 }) {
  return (
    <div style={{ width: 200, height: 4, background: 'rgba(120,120,128,0.2)', borderRadius: 99, position: 'relative' }}>
      <div style={{ position: 'absolute', left: 0, top: 0, bottom: 0, width: `${value*100}%`, background: '#0a84ff', borderRadius: 99 }}/>
      <div style={{ position: 'absolute', left: `calc(${value*100}% - 11px)`, top: -9, width: 22, height: 22, borderRadius: 99, background: '#fff', boxShadow: '0 2px 6px rgba(0,0,0,0.18), 0 0 0 0.5px rgba(0,0,0,0.06)' }}/>
    </div>
  );
}
function SegmentRow({ options, selected, dark }) {
  const t = ipadTheme(dark);
  return (
    <div style={{ padding: '10px 16px', borderBottom: `0.5px solid ${t.sep}` }}>
      <div style={{
        display: 'flex', borderRadius: 9, padding: 2,
        background: dark ? 'rgba(120,120,128,0.24)' : 'rgba(118,118,128,0.12)',
      }}>
        {options.map((o, i) => (
          <div key={i} style={{
            flex: 1, padding: '6px 10px', textAlign: 'center', borderRadius: 7,
            background: i === selected ? (dark ? '#636366' : '#fff') : 'transparent',
            fontSize: 13, fontWeight: 500, color: t.ink,
            boxShadow: i === selected ? '0 1px 2px rgba(0,0,0,0.08)' : 'none',
          }}>{o}</div>
        ))}
      </div>
    </div>
  );
}
function LatencySpark() {
  const pts = [10, 8, 9, 12, 7, 6, 8, 9, 7, 8, 10, 9, 8, 7, 8, 9, 8, 7, 6, 8];
  const max = 16;
  const w = 140, h = 24;
  const path = pts.map((v, i) => `${i === 0 ? 'M' : 'L'} ${(i/(pts.length-1))*w} ${h - (v/max)*h}`).join(' ');
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      <svg width={w} height={h} style={{ overflow: 'visible' }}>
        <path d={path} stroke="#30d158" strokeWidth="1.6" fill="none"/>
      </svg>
      <span style={{ fontSize: 13, color: '#000', fontVariantNumeric: 'tabular-nums' }}>8 ms</span>
    </div>
  );
}

window.SceneWelcome = SceneWelcome;
window.SceneDiscovery = SceneDiscovery;
window.ScenePairing = ScenePairing;
window.SceneLive = SceneLive;
window.SceneSettings = SceneSettings;
