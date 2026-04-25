import { useTranslation } from 'react-i18next'

const LANGS = ['en', 'tr']

const NAV = [
  { id: 'dashboard', icon: IconDashboard },
  { id: 'charts',    icon: IconCharts },
  { id: 'map',       icon: IconMap },
  { id: 'alerts',    icon: IconAlerts },
  { id: 'reports',   icon: IconReports },
  { id: 'api',       icon: IconApi },
  { id: 'settings',  icon: IconSettings, soon: true },
]

export default function Sidebar({ page, onNavigate, open, onClose, onLogout }) {
  const { t, i18n } = useTranslation()

  return (
    <>
      {/* Mobile backdrop */}
      {open && (
        <div
          className="fixed inset-0 z-30 bg-black/60 lg:hidden"
          onClick={onClose}
        />
      )}

      <aside className={[
        'fixed inset-y-0 left-0 z-40 w-[220px]',
        'bg-zinc-950 border-r border-zinc-800',
        'flex flex-col',
        'transition-transform duration-200 ease-in-out',
        open ? 'translate-x-0' : '-translate-x-full',
        'lg:translate-x-0',
      ].join(' ')}>

        {/* Brand */}
        <div className="h-14 flex items-center px-5 border-b border-zinc-800 shrink-0">
          <span className="text-zinc-100 font-thin tracking-[0.2em] text-sm select-none">
            ASTRAEUSIO
          </span>
        </div>

        {/* Nav */}
        <nav className="flex-1 overflow-y-auto py-2">
          {NAV.map(({ id, icon: Icon, soon }) => (
            <button
              key={id}
              onClick={() => { if (!soon) { onNavigate(id); onClose() } }}
              disabled={soon}
              className={[
                'w-full flex items-center gap-3 px-5 py-2.5',
                'text-sm font-mono tracking-wide transition-colors',
                page === id
                  ? 'text-zinc-100 bg-zinc-800'
                  : soon
                  ? 'text-zinc-700 cursor-not-allowed'
                  : 'text-zinc-500 hover:text-zinc-300 hover:bg-zinc-900',
              ].join(' ')}
            >
              <Icon />
              <span>{t(`nav.${id}`)}</span>
              {soon && (
                <span className="ml-auto text-[10px] text-zinc-700 font-mono">soon</span>
              )}
            </button>
          ))}
        </nav>

        {/* Footer */}
        <div className="border-t border-zinc-800 p-4 flex flex-col gap-3 shrink-0">
          <div className="flex items-center gap-1">
            {LANGS.map(lng => (
              <button
                key={lng}
                onClick={() => i18n.changeLanguage(lng)}
                className={`text-xs font-mono px-2 py-0.5 rounded transition-colors ${
                  i18n.language === lng
                    ? 'bg-zinc-700 text-zinc-100'
                    : 'text-zinc-500 hover:text-zinc-300'
                }`}
              >
                {lng.toUpperCase()}
              </button>
            ))}
          </div>
          <button
            onClick={onLogout}
            className="text-xs font-mono px-2 py-1 rounded text-left text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
          >
            {t('auth.logout')}
          </button>
        </div>
      </aside>
    </>
  )
}

/* ── Inline SVG icons (stroke-based, 16×16 viewport) ────────────────────── */

function IconDashboard() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <rect x="1" y="1" width="6" height="6" rx="1" />
      <rect x="9" y="1" width="6" height="6" rx="1" />
      <rect x="1" y="9" width="6" height="6" rx="1" />
      <rect x="9" y="9" width="6" height="6" rx="1" />
    </svg>
  )
}

function IconCharts() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <rect x="1" y="9" width="3" height="6" rx="0.5" />
      <rect x="6" y="5" width="3" height="10" rx="0.5" />
      <rect x="11" y="2" width="3" height="13" rx="0.5" />
    </svg>
  )
}

function IconMap() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <polygon points="1,3 5,1 11,3 15,1 15,13 11,15 5,13 1,15" />
      <line x1="5" y1="1" x2="5" y2="13" />
      <line x1="11" y1="3" x2="11" y2="15" />
    </svg>
  )
}

function IconAlerts() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M8 1a5 5 0 0 1 5 5c0 4 2 5 2 5H1s2-1 2-5a5 5 0 0 1 5-5z" />
      <path d="M6.5 13a1.5 1.5 0 0 0 3 0" />
    </svg>
  )
}

function IconReports() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <rect x="2" y="1" width="12" height="14" rx="1" />
      <line x1="5" y1="5" x2="11" y2="5" />
      <line x1="5" y1="8" x2="11" y2="8" />
      <line x1="5" y1="11" x2="8" y2="11" />
    </svg>
  )
}

function IconApi() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="4,5 1,8 4,11" />
      <polyline points="12,5 15,8 12,11" />
      <line x1="9.5" y1="3" x2="6.5" y2="13" />
    </svg>
  )
}

function IconSettings() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="8" cy="8" r="2.5" />
      <path d="M8 1v2M8 13v2M1 8h2M13 8h2
               M3.1 3.1l1.4 1.4M11.5 11.5l1.4 1.4
               M3.1 12.9l1.4-1.4M11.5 4.5l1.4-1.4" />
    </svg>
  )
}
