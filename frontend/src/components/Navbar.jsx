import { useState, useEffect } from 'react'
import { Link, useLocation } from 'react-router-dom'
import { useTranslation } from 'react-i18next'

const NAV_LINKS = [
  { to: '/products', key: 'landing.navProducts' },
  { to: '/pricing',  key: 'landing.navPricing'  },
  { to: '/docs',     key: 'landing.navDocs'      },
  { to: '/about',    key: 'landing.navAbout'     },
  { to: '/blog',     key: 'landing.navBlog'      },
  { to: '/status',   key: 'landing.navStatus'    },
]

function HamburgerIcon({ open }) {
  return (
    <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
      <line x1="3" y1="5"  x2="17" y2="5"  className={`transition-all duration-300 origin-center ${open ? 'opacity-0' : ''}`} />
      <line x1="3" y1="10" x2="17" y2="10" className={`transition-all duration-300 ${open ? 'opacity-0' : ''}`} />
      <line x1="3" y1="15" x2="17" y2="15" className={`transition-all duration-300 origin-center ${open ? 'opacity-0' : ''}`} />
      {open && <>
        <line x1="4" y1="4" x2="16" y2="16" className="transition-all duration-300" />
        <line x1="16" y1="4" x2="4"  y2="16" className="transition-all duration-300" />
      </>}
    </svg>
  )
}

export default function Navbar({ onSignIn }) {
  const { t, i18n } = useTranslation()
  const { pathname } = useLocation()
  const [open, setOpen] = useState(false)

  useEffect(() => { setOpen(false) }, [pathname])

  useEffect(() => {
    if (!open) return
    const onKey = e => { if (e.key === 'Escape') setOpen(false) }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open])

  useEffect(() => {
    document.body.style.overflow = open ? 'hidden' : ''
    return () => { document.body.style.overflow = '' }
  }, [open])

  function toggleLang() {
    i18n.changeLanguage(i18n.language === 'en' ? 'tr' : 'en')
  }

  return (
    <>
      <nav className="fixed top-0 left-0 right-0 z-50 flex items-center justify-between px-6 py-4 border-b border-white/5 bg-zinc-950/75 backdrop-blur-md">
        <Link to="/" className="font-thin tracking-[0.25em] text-sm select-none text-zinc-100 hover:text-white transition-colors">
          ASTRAEUSIO
        </Link>

        {/* Desktop links */}
        <div className="hidden md:flex items-center gap-10">
          {NAV_LINKS.map(({ to, key }) => {
            const active = pathname === to
            return (
              <div key={to} className="relative pb-0.5">
                <Link
                  to={to}
                  className={`text-sm transition-colors ${active ? 'text-zinc-100' : 'text-zinc-400 hover:text-zinc-100'}`}
                >
                  {t(key)}
                </Link>
                {active && (
                  <span className="absolute bottom-0 left-0 right-0 h-px bg-zinc-100 rounded-full" />
                )}
              </div>
            )
          })}
        </div>

        <div className="flex items-center gap-3">
          <button
            onClick={toggleLang}
            className="text-zinc-500 hover:text-zinc-300 text-xs font-mono px-2 py-1 rounded border border-zinc-800 hover:border-zinc-600 transition-colors"
          >
            {i18n.language === 'en' ? 'TR' : 'EN'}
          </button>
          <button
            onClick={onSignIn}
            className="hidden md:block text-zinc-300 hover:text-zinc-100 text-sm font-mono tracking-wide transition-colors"
          >
            {t('landing.nav')}
          </button>

          {/* Hamburger — mobile only */}
          <button
            onClick={() => setOpen(o => !o)}
            className="md:hidden text-zinc-400 hover:text-zinc-100 transition-colors p-1 -mr-1"
            aria-label="Toggle menu"
            aria-expanded={open}
          >
            <HamburgerIcon open={open} />
          </button>
        </div>
      </nav>

      {/* Mobile backdrop */}
      {open && (
        <div
          className="fixed inset-0 z-40 bg-black/60 backdrop-blur-sm md:hidden"
          onClick={() => setOpen(false)}
        />
      )}

      {/* Mobile drawer */}
      <div className={`fixed top-0 right-0 z-40 h-full w-72 bg-zinc-950 border-l border-zinc-800 flex flex-col transition-transform duration-300 ease-in-out md:hidden ${
        open ? 'translate-x-0' : 'translate-x-full'
      }`}>
        <div className="flex items-center justify-between px-6 py-4 border-b border-zinc-800">
          <span className="font-thin tracking-[0.25em] text-sm text-zinc-100 select-none">ASTRAEUSIO</span>
          <button
            onClick={() => setOpen(false)}
            className="text-zinc-500 hover:text-zinc-200 transition-colors p-1"
            aria-label="Close menu"
          >
            <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
              <line x1="3" y1="3" x2="15" y2="15" />
              <line x1="15" y1="3" x2="3"  y2="15" />
            </svg>
          </button>
        </div>

        <div className="flex flex-col px-6 py-6 gap-1 flex-1">
          {NAV_LINKS.map(({ to, key }) => {
            const active = pathname === to
            return (
              <Link
                key={to}
                to={to}
                className={`py-3 text-sm border-b border-zinc-800/60 transition-colors flex items-center justify-between ${
                  active ? 'text-zinc-100' : 'text-zinc-400 hover:text-zinc-100'
                }`}
              >
                {t(key)}
                {active && <span className="w-1.5 h-1.5 rounded-full bg-zinc-100 shrink-0" />}
              </Link>
            )
          })}
        </div>

        <div className="px-6 py-6 border-t border-zinc-800">
          <button
            onClick={() => { setOpen(false); onSignIn() }}
            className="w-full py-2.5 rounded-lg text-sm font-mono text-zinc-300 hover:text-zinc-100 border border-zinc-700 hover:border-zinc-500 transition-colors"
          >
            {t('landing.nav')}
          </button>
        </div>
      </div>
    </>
  )
}
