// SF Symbols-style icons inspired by iOS
// Stroke-based; pass color via `c` prop or use currentColor

const I = {
  // ── connection / network ──
  wifi: (s = 18, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <path d="M3 9c5-4.5 13-4.5 18 0" stroke={c} strokeWidth="2" strokeLinecap="round"/>
      <path d="M6 13c3.5-3 8.5-3 12 0" stroke={c} strokeWidth="2" strokeLinecap="round"/>
      <path d="M9 16.5c1.5-1.3 4.5-1.3 6 0" stroke={c} strokeWidth="2" strokeLinecap="round"/>
      <circle cx="12" cy="20" r="1.5" fill={c}/>
    </svg>
  ),
  link: (s = 18, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <path d="M10 14a4 4 0 005.66 0l3-3a4 4 0 10-5.66-5.66L11 7" stroke={c} strokeWidth="1.8" strokeLinecap="round"/>
      <path d="M14 10a4 4 0 00-5.66 0l-3 3a4 4 0 105.66 5.66L13 17" stroke={c} strokeWidth="1.8" strokeLinecap="round"/>
    </svg>
  ),
  bolt: (s = 16, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill={c}>
      <path d="M13 2L4 14h6l-1 8 9-12h-6l1-8z"/>
    </svg>
  ),
  power: (s = 18, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <path d="M12 3v9" stroke={c} strokeWidth="2" strokeLinecap="round"/>
      <path d="M6 7a8 8 0 1012 0" stroke={c} strokeWidth="2" strokeLinecap="round"/>
    </svg>
  ),
  // ── display modes ──
  extend: (s = 22, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 28 22" fill="none">
      <rect x="1" y="2" width="16" height="12" rx="1.5" stroke={c} strokeWidth="1.6"/>
      <rect x="14" y="6" width="13" height="10" rx="1.5" stroke={c} strokeWidth="1.6" fill="rgba(10,132,255,0.18)"/>
      <path d="M5 18h12" stroke={c} strokeWidth="1.6" strokeLinecap="round"/>
    </svg>
  ),
  mirror: (s = 22, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 28 22" fill="none">
      <rect x="1" y="2" width="13" height="10" rx="1.5" stroke={c} strokeWidth="1.6"/>
      <rect x="14" y="2" width="13" height="10" rx="1.5" stroke={c} strokeWidth="1.6" fill="rgba(10,132,255,0.18)"/>
      <path d="M14 7h0M5 18h18" stroke={c} strokeWidth="1.6" strokeLinecap="round" strokeDasharray="2 2"/>
    </svg>
  ),
  pencil: (s = 22, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <path d="M3 21l3-1 12-12-2-2L4 18l-1 3z" stroke={c} strokeWidth="1.6" strokeLinejoin="round"/>
      <path d="M14 6l2 2" stroke={c} strokeWidth="1.6"/>
      <rect x="16.5" y="2.5" width="5" height="3" rx="1" transform="rotate(45 16.5 2.5)" stroke={c} strokeWidth="1.6"/>
    </svg>
  ),
  // ── ui ──
  chevR: (s = 14, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 14 14" fill="none">
      <path d="M5 2l5 5-5 5" stroke={c} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  chevL: (s = 14, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 14 14" fill="none">
      <path d="M9 2L4 7l5 5" stroke={c} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  chevD: (s = 12, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 14 14" fill="none">
      <path d="M2 5l5 5 5-5" stroke={c} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  close: (s = 14, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 14 14" fill="none">
      <path d="M3 3l8 8M11 3l-8 8" stroke={c} strokeWidth="2" strokeLinecap="round"/>
    </svg>
  ),
  check: (s = 14, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 14 14" fill="none">
      <path d="M2 7l3.5 3.5L12 4" stroke={c} strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  plus: (s = 14, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 14 14" fill="none">
      <path d="M7 2v10M2 7h10" stroke={c} strokeWidth="2" strokeLinecap="round"/>
    </svg>
  ),
  gear: (s = 18, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <circle cx="12" cy="12" r="3" stroke={c} strokeWidth="1.8"/>
      <path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1" stroke={c} strokeWidth="1.6" strokeLinecap="round"/>
    </svg>
  ),
  search: (s = 16, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 18 18" fill="none">
      <circle cx="8" cy="8" r="5" stroke={c} strokeWidth="1.8"/>
      <path d="M12 12l4 4" stroke={c} strokeWidth="1.8" strokeLinecap="round"/>
    </svg>
  ),
  drag: (s = 16, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 16 16" fill={c}>
      <circle cx="6" cy="3" r="1.2"/><circle cx="10" cy="3" r="1.2"/>
      <circle cx="6" cy="8" r="1.2"/><circle cx="10" cy="8" r="1.2"/>
      <circle cx="6" cy="13" r="1.2"/><circle cx="10" cy="13" r="1.2"/>
    </svg>
  ),
  // ── system / status ──
  battery: (s = 24, c = 'currentColor', fill = 0.7) => (
    <svg width={s} height={14} viewBox="0 0 27 13">
      <rect x="0.5" y="0.5" width="23" height="12" rx="3.5" stroke={c} strokeOpacity="0.45" fill="none"/>
      <rect x="2" y="2" width={20*fill} height="9" rx="2" fill={c}/>
      <path d="M25 4.5V8.5C25.8 8.2 26.5 7.2 26.5 6.5C26.5 5.8 25.8 4.8 25 4.5Z" fill={c} fillOpacity="0.4"/>
    </svg>
  ),
  signal: (s = 18, c = 'currentColor') => (
    <svg width={s} height={12} viewBox="0 0 19 12">
      <rect x="0" y="7.5" width="3.2" height="4.5" rx="0.7" fill={c}/>
      <rect x="4.8" y="5" width="3.2" height="7" rx="0.7" fill={c}/>
      <rect x="9.6" y="2.5" width="3.2" height="9.5" rx="0.7" fill={c}/>
      <rect x="14.4" y="0" width="3.2" height="12" rx="0.7" fill={c}/>
    </svg>
  ),
  warn: (s = 16, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <path d="M12 3l10 18H2L12 3z" stroke={c} strokeWidth="1.8" strokeLinejoin="round"/>
      <path d="M12 10v5" stroke={c} strokeWidth="2" strokeLinecap="round"/>
      <circle cx="12" cy="18" r="1" fill={c}/>
    </svg>
  ),
  refresh: (s = 16, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <path d="M3 12a9 9 0 0115-6.7L21 8" stroke={c} strokeWidth="1.8" strokeLinecap="round"/>
      <path d="M21 3v5h-5" stroke={c} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"/>
      <path d="M21 12a9 9 0 01-15 6.7L3 16" stroke={c} strokeWidth="1.8" strokeLinecap="round"/>
      <path d="M3 21v-5h5" stroke={c} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  // ── tools (paint/brush) ──
  brush: (s = 18, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <path d="M16 4l4 4-9 9-5 1 1-5 9-9z" stroke={c} strokeWidth="1.6" strokeLinejoin="round"/>
    </svg>
  ),
  eraser: (s = 18, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <path d="M14 4l6 6-9 9H5l-2-2 11-13z" stroke={c} strokeWidth="1.6" strokeLinejoin="round"/>
      <path d="M9 9l6 6" stroke={c} strokeWidth="1.6"/>
    </svg>
  ),
  hand: (s = 18, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <path d="M9 11V5a1.5 1.5 0 013 0v6m0 0V4a1.5 1.5 0 013 0v7m0 0V6a1.5 1.5 0 013 0v8c0 4-3 7-7 7s-7-3-7-7v-3a1.5 1.5 0 113 0v3" stroke={c} strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  // ── pc / desktop chrome ──
  win: (s = 14, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 14 14" fill={c}>
      <rect x="1" y="1" width="5.5" height="5.5"/>
      <rect x="7.5" y="1" width="5.5" height="5.5"/>
      <rect x="1" y="7.5" width="5.5" height="5.5"/>
      <rect x="7.5" y="7.5" width="5.5" height="5.5"/>
    </svg>
  ),
  monitor: (s = 18, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <rect x="2" y="3" width="20" height="13" rx="1.5" stroke={c} strokeWidth="1.6"/>
      <path d="M9 20h6M12 16v4" stroke={c} strokeWidth="1.6" strokeLinecap="round"/>
    </svg>
  ),
  ipad: (s = 18, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <rect x="4" y="3" width="16" height="18" rx="2" stroke={c} strokeWidth="1.6"/>
      <circle cx="12" cy="18" r="0.8" fill={c}/>
    </svg>
  ),
  arrowsLR: (s = 18, c = 'currentColor') => (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <path d="M7 8l-4 4 4 4M17 8l4 4-4 4M3 12h18" stroke={c} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
};

window.I = I;
