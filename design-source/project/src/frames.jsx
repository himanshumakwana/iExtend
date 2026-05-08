// Device frames: iPad Pro 11" (landscape) + PC monitor with Windows-ish chrome.

const { useState, useEffect, useRef, useMemo } = React;

// ─────────────────────────────────────────────────────────
// iPad Pro 11" landscape — 1194 × 834 pt @ scaled. Base 720 wide.
// ─────────────────────────────────────────────────────────
function IPad({ width = 760, dark = false, children, scale = 1, label }) {
  // 11" iPad aspect 1194:834 ≈ 1.4317
  const aspect = 1194 / 834;
  const w = width;
  const h = Math.round(w / aspect);

  return (
    <div data-screen-label={label} style={{ position: 'relative', transform: `scale(${scale})`, transformOrigin: 'center' }}>
      <div className="ipad-frame" style={{ width: w, height: h }}>
        <div className="ipad-cam" />
        <div className="ipad-screen" style={{ width: '100%', height: '100%', background: dark ? '#000' : '#f2f2f7' }}>
          {children}
        </div>
      </div>
    </div>
  );
}

// iPad status bar — landscape (skinny, ears on sides)
function IPadStatusBar({ dark = false, time = '9:41', wifi = true, battery = 0.78, latencyMs }) {
  const c = dark ? '#fff' : '#000';
  const muted = dark ? 'rgba(255,255,255,0.7)' : 'rgba(0,0,0,0.65)';
  return (
    <div style={{
      position: 'absolute', top: 0, left: 0, right: 0, height: 28, zIndex: 30,
      display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      padding: '0 22px',
      fontFamily: '-apple-system, "SF Pro Text", system-ui',
      fontSize: 13, fontWeight: 600, color: c, letterSpacing: -0.1,
      pointerEvents: 'none',
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span style={{ fontVariantNumeric: 'tabular-nums' }}>{time}</span>
        {latencyMs !== undefined && (
          <span style={{
            fontSize: 11, fontWeight: 500, color: muted,
            display: 'inline-flex', alignItems: 'center', gap: 4,
            padding: '1px 6px', borderRadius: 6,
            background: dark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.05)',
          }}>
            <span style={{ width: 5, height: 5, borderRadius: 99, background: '#30d158' }} />
            iExtend · {latencyMs}ms
          </span>
        )}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
        {wifi && (
          <svg width="14" height="10" viewBox="0 0 14 10" fill="none">
            <path d="M1 4.5C2.5 3 4.6 2 7 2s4.5 1 6 2.5" stroke={c} strokeWidth="1.3" strokeLinecap="round"/>
            <path d="M3 6.5c1-1 2.4-1.6 4-1.6s3 0.6 4 1.6" stroke={c} strokeWidth="1.3" strokeLinecap="round"/>
            <circle cx="7" cy="8.5" r="1.1" fill={c}/>
          </svg>
        )}
        <svg width="22" height="11" viewBox="0 0 27 13">
          <rect x="0.5" y="0.5" width="23" height="12" rx="3.5" stroke={c} strokeOpacity="0.45" fill="none"/>
          <rect x="2" y="2" width={20*battery} height="9" rx="2" fill={c}/>
          <path d="M25 4.5V8.5C25.8 8.2 26.5 7.2 26.5 6.5C26.5 5.8 25.8 4.8 25 4.5Z" fill={c} fillOpacity="0.4"/>
        </svg>
      </div>
    </div>
  );
}

// Home indicator (landscape — bottom)
function IPadHomeIndicator({ dark = false }) {
  return (
    <div style={{
      position: 'absolute', bottom: 6, left: 0, right: 0,
      display: 'flex', justifyContent: 'center', zIndex: 60, pointerEvents: 'none',
    }}>
      <div style={{
        width: 134, height: 5, borderRadius: 100,
        background: dark ? 'rgba(255,255,255,0.6)' : 'rgba(0,0,0,0.28)',
      }} />
    </div>
  );
}

// ─────────────────────────────────────────────────────────
// PC Monitor — slightly tilted, 16:9, runs the companion app
// ─────────────────────────────────────────────────────────
function Monitor({ width = 560, children, label }) {
  const aspect = 16 / 10; // a touch taller than 16:9 to feel like a thunderbolt display
  const w = width;
  const h = Math.round(w / aspect);
  return (
    <div data-screen-label={label} className="monitor-wrap">
      <div className="monitor" style={{ width: w + 28 }}>
        <div className="monitor-screen" style={{ width: w, height: h }}>
          {children}
        </div>
      </div>
      <div className="monitor-stand" />
      <div className="monitor-base" />
    </div>
  );
}

// Windows-ish chrome (titlebar) for the companion app
function WinChrome({ title = 'iExtend', accent = '#0a84ff', children }) {
  return (
    <div style={{
      width: '100%', height: '100%',
      display: 'flex', flexDirection: 'column',
      background: '#f3f3f3', // win11-ish
      fontFamily: '"Segoe UI Variable", "Segoe UI", system-ui',
      color: '#1a1a1a',
    }}>
      {/* titlebar */}
      <div style={{
        height: 32, display: 'flex', alignItems: 'center',
        background: '#ffffff', borderBottom: '1px solid rgba(0,0,0,0.06)',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '0 12px', flex: 1 }}>
          <div style={{
            width: 14, height: 14, borderRadius: 4,
            background: `linear-gradient(140deg, ${accent} 0%, #5e5ce6 100%)`,
          }}/>
          <span style={{ fontSize: 12, color: '#1a1a1a' }}>{title}</span>
        </div>
        <div style={{ display: 'flex', height: '100%' }}>
          {[
            <svg key="m" width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="#5a5a5a" strokeWidth="1"/></svg>,
            <svg key="b" width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" stroke="#5a5a5a" strokeWidth="1" fill="none"/></svg>,
            <svg key="x" width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="#5a5a5a" strokeWidth="1"/></svg>,
          ].map((el, i) => (
            <div key={i} style={{
              width: 46, height: '100%', display: 'grid', placeItems: 'center',
              background: i === 2 && false ? '#e81123' : 'transparent',
            }}>{el}</div>
          ))}
        </div>
      </div>
      <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>{children}</div>
    </div>
  );
}

// Wallpaper component used as a fake desktop "behind" the iExtend windows when needed
function FakeDesktop({ accent = '#0a84ff' }) {
  return (
    <div style={{
      position: 'absolute', inset: 0,
      background: `
        radial-gradient(900px 500px at 30% 20%, rgba(94,92,230,0.55), transparent 60%),
        radial-gradient(700px 500px at 80% 80%, rgba(10,132,255,0.45), transparent 60%),
        linear-gradient(180deg, #0c1430 0%, #131736 60%, #0a0d22 100%)`,
    }}/>
  );
}

window.IPad = IPad;
window.IPadStatusBar = IPadStatusBar;
window.IPadHomeIndicator = IPadHomeIndicator;
window.Monitor = Monitor;
window.WinChrome = WinChrome;
window.FakeDesktop = FakeDesktop;
