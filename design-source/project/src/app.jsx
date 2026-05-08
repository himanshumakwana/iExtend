// Main app — Figma-style design canvas, DCArtboard direct children of DCSection.

const { useState, useEffect } = React;

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "dark": false,
  "toolbarPos": "bottom",
  "density": "regular",
  "conn": "live"
}/*EDITMODE-END*/;

const IPAD_W = 720;
const PC_W = 540;
const IPAD_H = Math.round((IPAD_W + 28) / (1194/834)) + 60;
const PC_H = Math.round(PC_W / (16/10)) + 90;

function ipadInner(scene, dark, conn, toolbarPos, density, label) {
  const ipadDark = scene === 'live' ? true : dark;
  return (
    <div style={{ width: '100%', height: '100%', display: 'grid', placeItems: 'center', background: '#0f1115' }}>
      <IPad width={IPAD_W} dark={ipadDark} label={label}>
        {scene === 'welcome'  && <SceneWelcome dark={ipadDark}/>}
        {scene === 'discover' && <SceneDiscovery dark={ipadDark}/>}
        {scene === 'pair'     && <ScenePairing dark={ipadDark}/>}
        {scene === 'live'     && <SceneLive dark conn={conn} toolbarPos={toolbarPos} density={density}/>}
        {scene === 'settings' && <SceneSettings dark={ipadDark}/>}
      </IPad>
    </div>
  );
}

function pcInner(scene, conn, label) {
  return (
    <div style={{ width: '100%', height: '100%', display: 'grid', placeItems: 'center', background: '#0f1115' }}>
      <Monitor width={PC_W} label={label}>
        {scene === 'welcome' && <PCWelcome/>}
        {scene === 'devices' && <PCDevices/>}
        {scene === 'pair'    && <PCPair/>}
        {scene === 'live'    && <PCLive conn={conn}/>}
        {scene === 'error'   && <PCError/>}
      </Monitor>
    </div>
  );
}

function App() {
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const iw = IPAD_W + 28, ih = IPAD_H;
  const pw = PC_W + 28,   ph = PC_H;

  return (
    <>
      <DesignCanvas>
        <DCSection id="onboarding" title="Onboarding" subtitle="First-launch · pair iPad with PC over Wi-Fi">
          <DCArtboard id="ipad-welcome"  label="iPad · Welcome"   width={iw} height={ih}>{ipadInner('welcome', t.dark, t.conn, t.toolbarPos, t.density, 'iPad · Welcome')}</DCArtboard>
          <DCArtboard id="pc-welcome"    label="PC · Searching"   width={pw} height={ph}>{pcInner('welcome', t.conn, 'PC · Searching')}</DCArtboard>
          <DCArtboard id="ipad-discover" label="iPad · Discover"  width={iw} height={ih}>{ipadInner('discover', t.dark, t.conn, t.toolbarPos, t.density, 'iPad · Discover')}</DCArtboard>
          <DCArtboard id="pc-devices"    label="PC · Devices"     width={pw} height={ph}>{pcInner('devices', t.conn, 'PC · Devices')}</DCArtboard>
          <DCArtboard id="ipad-pair"     label="iPad · Pair"      width={iw} height={ih}>{ipadInner('pair', t.dark, t.conn, t.toolbarPos, t.density, 'iPad · Pair')}</DCArtboard>
          <DCArtboard id="pc-pair"       label="PC · Show code"   width={pw} height={ph}>{pcInner('pair', t.conn, 'PC · Show code')}</DCArtboard>
        </DCSection>

        <DCSection id="connected" title="Connected · live extended desktop" subtitle="Floating glass toolbar · pencil · latency · end">
          <DCArtboard id="ipad-connecting" label="iPad · Connecting"      width={iw} height={ih}>{ipadInner('live', t.dark, 'connecting', t.toolbarPos, t.density, 'iPad · Connecting')}</DCArtboard>
          <DCArtboard id="ipad-live"       label="iPad · Live (extended)" width={iw} height={ih}>{ipadInner('live', t.dark, 'live', t.toolbarPos, t.density, 'iPad · Live')}</DCArtboard>
          <DCArtboard id="pc-live"         label="PC · Arrangement"        width={pw} height={ph}>{pcInner('live', 'live', 'PC · Arrangement')}</DCArtboard>
        </DCSection>

        <DCSection id="settings" title="Settings & errors" subtitle="iPadOS-style preferences · disconnect recovery">
          <DCArtboard id="ipad-settings" label="iPad · Settings"     width={iw} height={ih}>{ipadInner('settings', t.dark, t.conn, t.toolbarPos, t.density, 'iPad · Settings')}</DCArtboard>
          <DCArtboard id="ipad-error"    label="iPad · Disconnected" width={iw} height={ih}>{ipadInner('live', t.dark, 'error', t.toolbarPos, t.density, 'iPad · Disconnected')}</DCArtboard>
          <DCArtboard id="pc-error"      label="PC · Connection lost" width={pw} height={ph}>{pcInner('error', 'error', 'PC · Connection lost')}</DCArtboard>
        </DCSection>

        <DCSection id="toolbar" title="Floating toolbar variants" subtitle="Try Tweaks → Toolbar position / Density">
          <DCArtboard id="ipad-tb-bottom" label="Bottom · regular" width={iw} height={ih}>{ipadInner('live', t.dark, 'live', 'bottom', 'regular', 'Bottom · regular')}</DCArtboard>
          <DCArtboard id="ipad-tb-top"    label="Top · compact"    width={iw} height={ih}>{ipadInner('live', t.dark, 'live', 'top', 'compact', 'Top · compact')}</DCArtboard>
          <DCArtboard id="ipad-tb-left"   label="Left · comfy"     width={iw} height={ih}>{ipadInner('live', t.dark, 'live', 'left', 'comfy', 'Left · comfy')}</DCArtboard>
        </DCSection>
      </DesignCanvas>

      <TweaksPanel>
        <TweakSection label="Theme"/>
        <TweakToggle label="Dark mode (iPad UI)" value={t.dark} onChange={(v) => setTweak('dark', v)}/>
        <TweakSection label="Live screen"/>
        <TweakRadio label="Toolbar position" value={t.toolbarPos} options={['top', 'bottom', 'left']} onChange={(v) => setTweak('toolbarPos', v)}/>
        <TweakRadio label="Density" value={t.density} options={['compact', 'regular', 'comfy']} onChange={(v) => setTweak('density', v)}/>
        <TweakRadio label="Connection" value={t.conn} options={['live', 'connecting', 'error']} onChange={(v) => setTweak('conn', v)}/>
      </TweaksPanel>
    </>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App/>);
