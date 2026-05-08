// Companion PC desktop app scenes (Windows-ish chrome).
// Each one mirrors the iPad's current state.

function PCAppShell({ children, sidebar = 'home' }) {
  const items = [
    { k: 'home',    label: 'Home',     ic: I.ipad(16, '#1a1a1a') },
    { k: 'devices', label: 'Devices',  ic: I.monitor(16, '#1a1a1a') },
    { k: 'display', label: 'Display',  ic: I.extend(16, '#1a1a1a') },
    { k: 'pencil',  label: 'Pencil',   ic: I.pencil(16, '#1a1a1a') },
    { k: 'about',   label: 'About',    ic: I.gear(16, '#1a1a1a') },
  ];
  return (
    <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
      <div style={{
        width: 160, background: '#fafafa', borderRight: '1px solid rgba(0,0,0,0.06)',
        padding: '14px 8px', display: 'flex', flexDirection: 'column', gap: 2,
      }}>
        <div style={{ fontSize: 10, fontWeight: 600, color: '#666', padding: '4px 10px 6px', letterSpacing: 0.06, textTransform: 'uppercase' }}>iExtend</div>
        {items.map(it => (
          <div key={it.k} style={{
            display: 'flex', alignItems: 'center', gap: 10, padding: '7px 10px', borderRadius: 6,
            background: it.k === sidebar ? 'rgba(10,132,255,0.12)' : 'transparent',
            color: it.k === sidebar ? '#0a84ff' : '#1a1a1a',
            fontSize: 12, fontWeight: it.k === sidebar ? 600 : 500,
          }}>
            <span style={{ display: 'inline-flex' }}>{it.ic}</span> {it.label}
          </div>
        ))}
      </div>
      <div style={{ flex: 1, minHeight: 0, overflow: 'hidden' }}>{children}</div>
    </div>
  );
}

function PCWelcome() {
  return (
    <PCAppShell sidebar="home">
      <div style={{ padding: '14px 18px', height: '100%', display: 'flex', flexDirection: 'column', gap: 10, boxSizing: 'border-box', overflow: 'hidden' }}>
        <div style={{ fontSize: 17, fontWeight: 700, letterSpacing: -0.01 }}>Welcome to iExtend</div>
        <div style={{ fontSize: 11, color: '#555', maxWidth: 380, lineHeight: 1.4 }}>
          Use your iPad as a wireless second display. Open iExtend on your iPad — we'll pair automatically.
        </div>
        <div style={{
          padding: 10, borderRadius: 8,
          background: 'linear-gradient(140deg, rgba(10,132,255,0.06), rgba(94,92,230,0.06))',
          border: '1px solid rgba(10,132,255,0.2)',
          display: 'flex', alignItems: 'center', gap: 14,
        }}>
          <div style={{ width: 32, height: 32, borderRadius: 8, background: '#fff', display: 'grid', placeItems: 'center', border: '1px solid rgba(0,0,0,0.06)' }}>
            {I.wifi(16, '#0a84ff')}
          </div>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 11.5, fontWeight: 600 }}>Searching for iPads on HomeNet 5G</div>
            <div style={{ fontSize: 10, color: '#666' }}>Same Wi‑Fi · auto‑pair on launch</div>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 5, fontSize: 10, color: '#0a84ff' }}>
            <span style={{ width: 5, height: 5, borderRadius: 99, background: '#0a84ff', animation: 'blink 1s infinite' }}/> Scanning…
          </div>
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {[
            { t: 'Pair via QR', s: 'Show a code on screen' },
            { t: 'Pair via PIN', s: '4‑digit one‑time code' },
            { t: 'Use USB‑C', s: 'Plug in for lowest latency' },
            { t: 'Manual IP', s: 'Connect across subnets' },
          ].map((c, i) => (
            <div key={i} style={{
              padding: '5px 10px', borderRadius: 6, background: '#fff', border: '1px solid rgba(0,0,0,0.08)',
              display: 'flex', alignItems: 'baseline', gap: 6,
            }}>
              <div style={{ fontSize: 11, fontWeight: 600 }}>{c.t}</div>
              <div style={{ fontSize: 10, color: '#666' }}>· {c.s}</div>
            </div>
          ))}
        </div>
      </div>
    </PCAppShell>
  );
}

function PCDevices() {
  const list = [
    { name: "Aman's iPad Pro 11\"", sub: 'iPadOS 26 · 192.168.1.78', ms: 8, sel: true },
    { name: "Studio iPad Air",       sub: 'iPadOS 26 · 192.168.1.81', ms: 14 },
  ];
  return (
    <PCAppShell sidebar="devices">
      <div style={{ padding: 22, height: '100%', display: 'flex', flexDirection: 'column' }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <div>
            <div style={{ fontSize: 18, fontWeight: 700 }}>Available iPads</div>
            <div style={{ fontSize: 11, color: '#666', marginTop: 2 }}>2 devices on HomeNet 5G</div>
          </div>
          <div style={{
            padding: '5px 10px', borderRadius: 6, background: '#0a84ff', color: '#fff', fontSize: 11, fontWeight: 600,
            display: 'inline-flex', alignItems: 'center', gap: 6,
          }}>{I.refresh(12, '#fff')} Rescan</div>
        </div>
        <div style={{ marginTop: 14, border: '1px solid rgba(0,0,0,0.08)', borderRadius: 8, overflow: 'hidden', background: '#fff' }}>
          {list.map((d, i) => (
            <div key={i} style={{
              display: 'flex', alignItems: 'center', gap: 12, padding: '10px 14px',
              borderTop: i ? '1px solid rgba(0,0,0,0.06)' : 'none',
              background: d.sel ? 'rgba(10,132,255,0.08)' : '#fff',
            }}>
              <div style={{ width: 32, height: 32, borderRadius: 6, background: 'rgba(0,0,0,0.04)', display: 'grid', placeItems: 'center' }}>{I.ipad(18, '#1a1a1a')}</div>
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 13, fontWeight: 600 }}>{d.name}</div>
                <div style={{ fontSize: 10.5, color: '#666' }}>{d.sub}</div>
              </div>
              <div style={{ fontSize: 11, color: '#666', fontVariantNumeric: 'tabular-nums', marginRight: 8 }}>~{d.ms} ms</div>
              {d.sel
                ? <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, fontSize: 11, color: '#30a04e', fontWeight: 600 }}>
                    <span style={{ width: 6, height: 6, borderRadius: 99, background: '#30a04e' }}/> Connected
                  </span>
                : <span style={{ padding: '4px 10px', borderRadius: 5, background: '#0a84ff', color: '#fff', fontSize: 11, fontWeight: 600 }}>Connect</span>}
            </div>
          ))}
        </div>

        <div style={{ marginTop: 18, fontSize: 11, color: '#666', display: 'inline-flex', alignItems: 'center', gap: 6 }}>
          {I.bolt(12, '#666')} Tip: keep this window open in the system tray to auto‑connect on launch.
        </div>
      </div>
    </PCAppShell>
  );
}

function PCPair() {
  return (
    <PCAppShell sidebar="devices">
      <div style={{ padding: 22, height: '100%', display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div style={{ fontSize: 18, fontWeight: 700 }}>Pair this PC with an iPad</div>
        <div style={{ fontSize: 11, color: '#666' }}>Open iExtend on your iPad and either scan the code or enter the PIN.</div>
        <div style={{ display: 'grid', gridTemplateColumns: '0.85fr 1fr', gap: 10, marginTop: 6 }}>
          <div style={{ padding: 12, borderRadius: 8, background: '#fff', border: '1px solid rgba(0,0,0,0.08)', textAlign: 'center' }}>
            <div style={{ fontSize: 10, color: '#666', textTransform: 'uppercase', letterSpacing: 0.05, fontWeight: 600 }}>QR code</div>
            <div style={{ width: 110, height: 110, margin: '10px auto', display: 'grid', gridTemplateColumns: 'repeat(15,1fr)', gap: 1, background: '#fff' }}>
              {Array.from({ length: 15*15 }).map((_, i) => {
                const x = i%15, y = Math.floor(i/15);
                const inFinder = (xi,yi) => (x>=xi && x<=xi+4 && y>=yi && y<=yi+4) && !(x>xi+1 && x<xi+3 && y>yi+1 && y<yi+3);
                const finder = inFinder(0,0)||inFinder(10,0)||inFinder(0,10);
                const r = (i*9301+49297) % 233280 / 233280;
                return <div key={i} style={{ background: finder || r>0.55 ? '#000' : '#fff' }}/>;
              })}
            </div>
          </div>
          <div style={{ padding: 12, borderRadius: 8, background: '#fff', border: '1px solid rgba(0,0,0,0.08)' }}>
            <div style={{ fontSize: 10, color: '#666', textTransform: 'uppercase', letterSpacing: 0.05, fontWeight: 600, textAlign: 'center' }}>PIN</div>
            <div style={{ display: 'flex', justifyContent: 'center', gap: 5, marginTop: 10 }}>
              {['4','7','2','9'].map((d, i) => (
                <div key={i} style={{
                  width: 30, height: 40, borderRadius: 6, background: 'rgba(0,0,0,0.04)',
                  display: 'grid', placeItems: 'center', fontFamily: '"Segoe UI", system-ui',
                  fontSize: 19, fontWeight: 600,
                }}>{d}</div>
              ))}
            </div>
            <div style={{ fontSize: 10, color: '#666', textAlign: 'center', marginTop: 8 }}>Expires in <b>0:54</b></div>
            <div style={{ marginTop: 8, fontSize: 10, color: '#666' }}>Aman's iPad Pro 11" is requesting to pair.</div>
            <div style={{ marginTop: 6, display: 'flex', gap: 6 }}>
              <div style={{ flex: 1, padding: '5px', borderRadius: 5, background: 'rgba(0,0,0,0.05)', fontSize: 10.5, textAlign: 'center', fontWeight: 600 }}>Deny</div>
              <div style={{ flex: 1, padding: '5px', borderRadius: 5, background: '#0a84ff', color: '#fff', fontSize: 10.5, textAlign: 'center', fontWeight: 600 }}>Allow</div>
            </div>
          </div>
        </div>
      </div>
    </PCAppShell>
  );
}

function PCLive({ conn = 'live' }) {
  // Show a desktop arrangement diagram
  return (
    <PCAppShell sidebar="display">
      <div style={{ padding: 22, height: '100%', display: 'flex', flexDirection: 'column' }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <div>
            <div style={{ fontSize: 18, fontWeight: 700 }}>Display arrangement</div>
            <div style={{ fontSize: 11, color: '#666' }}>Drag to position. iPad is your <b>extended</b> display.</div>
          </div>
          <span style={{
            display: 'inline-flex', alignItems: 'center', gap: 6, fontSize: 11, color: '#30a04e', fontWeight: 600,
            padding: '4px 10px', borderRadius: 99, background: 'rgba(48,209,88,0.12)',
          }}>
            <span style={{ width: 6, height: 6, borderRadius: 99, background: '#30a04e' }}/>
            {conn === 'live' ? 'Connected · 8 ms' : conn === 'connecting' ? 'Connecting…' : 'Disconnected'}
          </span>
        </div>
        {/* arrangement canvas */}
        <div style={{
          marginTop: 14, flex: 1, borderRadius: 8,
          background: 'repeating-linear-gradient(45deg, #fafafa 0 12px, #f3f3f3 12px 24px)',
          border: '1px solid rgba(0,0,0,0.08)',
          display: 'grid', placeItems: 'center', position: 'relative',
        }}>
          <div style={{ display: 'flex', alignItems: 'flex-end', gap: 12 }}>
            <div style={{ width: 130, height: 80, borderRadius: 4, background: '#1a1c20', position: 'relative', boxShadow: '0 4px 10px rgba(0,0,0,0.18)' }}>
              <div style={{ position: 'absolute', inset: 4, background: 'linear-gradient(140deg,#0c1430,#5e5ce6)', borderRadius: 2 }}/>
              <div style={{ position: 'absolute', bottom: -14, left: '50%', transform: 'translateX(-50%)', fontSize: 10, color: '#666', fontWeight: 600 }}>1 · Main</div>
            </div>
            <div style={{
              width: 92, height: 68, borderRadius: 4, background: '#1a1c20', position: 'relative',
              boxShadow: '0 4px 10px rgba(0,0,0,0.18), 0 0 0 2px #0a84ff',
            }}>
              <div style={{ position: 'absolute', inset: 4, background: 'linear-gradient(140deg,#0a84ff,#5e5ce6)', borderRadius: 2 }}/>
              <div style={{ position: 'absolute', bottom: -14, left: '50%', transform: 'translateX(-50%)', fontSize: 10, color: '#0a84ff', fontWeight: 700, whiteSpace: 'nowrap' }}>2 · iPad</div>
            </div>
          </div>
          <div style={{ position: 'absolute', left: 14, top: 14, fontSize: 10, color: '#888' }}>1920×1200 · 120 Hz</div>
        </div>
        {/* readouts */}
        <div style={{ marginTop: 14, display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 8 }}>
          {[
            ['Latency', '8 ms', '#30a04e'],
            ['Frame rate', '118 fps', '#1a1a1a'],
            ['Bandwidth', '42 Mbps', '#1a1a1a'],
            ['Battery (iPad)', '78%', '#1a1a1a'],
          ].map(([k, v, c], i) => (
            <div key={i} style={{
              padding: 10, borderRadius: 6, background: '#fff', border: '1px solid rgba(0,0,0,0.06)',
            }}>
              <div style={{ fontSize: 10, color: '#666', textTransform: 'uppercase', letterSpacing: 0.04 }}>{k}</div>
              <div style={{ fontSize: 14, fontWeight: 700, color: c, marginTop: 2 }}>{v}</div>
            </div>
          ))}
        </div>
      </div>
    </PCAppShell>
  );
}

function PCError() {
  return (
    <PCAppShell sidebar="devices">
      <div style={{ padding: 22, height: '100%', display: 'flex', flexDirection: 'column', alignItems: 'flex-start', gap: 12 }}>
        <div style={{
          display: 'inline-flex', alignItems: 'center', gap: 8, padding: '5px 10px', borderRadius: 99,
          background: 'rgba(255,69,58,0.12)', color: '#c0382e', fontSize: 11, fontWeight: 600,
        }}>
          {I.warn(12, '#c0382e')} Connection lost
        </div>
        <div style={{ fontSize: 18, fontWeight: 700 }}>Aman's iPad Pro 11" disconnected</div>
        <div style={{ fontSize: 12, color: '#666', maxWidth: 380, lineHeight: 1.5 }}>
          We lost the link 4 seconds ago. The iPad will reconnect automatically when it's back on the same Wi‑Fi.
        </div>
        <div style={{ marginTop: 8, display: 'flex', gap: 8 }}>
          <div style={{ padding: '6px 14px', borderRadius: 6, background: '#0a84ff', color: '#fff', fontSize: 12, fontWeight: 600 }}>Try again</div>
          <div style={{ padding: '6px 14px', borderRadius: 6, background: '#fff', border: '1px solid rgba(0,0,0,0.12)', fontSize: 12, fontWeight: 600 }}>Forget device</div>
        </div>
        {/* log */}
        <div style={{ marginTop: 16, width: '100%', flex: 1, background: '#0f1115', borderRadius: 8, padding: 12, fontFamily: 'ui-monospace, "Cascadia Mono", monospace', fontSize: 10.5, color: '#9ec0ff', overflow: 'hidden' }}>
          <div style={{ color: '#666' }}>[18:02:14]</div>
          <div>conn: <span style={{ color: '#30d158' }}>OK</span> · 192.168.1.78 · 8ms · 118fps</div>
          <div>conn: <span style={{ color: '#ff9f0a' }}>WARN</span> · jitter 38ms (was 4ms)</div>
          <div>conn: <span style={{ color: '#ff453a' }}>LOST</span> · keepalive timeout · retry in 3s</div>
          <div>conn: retrying… <span style={{ background: '#9ec0ff', color: '#000', padding: '0 2px' }}> </span></div>
        </div>
      </div>
    </PCAppShell>
  );
}

window.PCWelcome = PCWelcome;
window.PCDevices = PCDevices;
window.PCPair = PCPair;
window.PCLive = PCLive;
window.PCError = PCError;
