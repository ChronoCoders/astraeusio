import { Link, useLocation } from 'react-router-dom'
import { useTranslation } from 'react-i18next'

const NAV_LINKS = [
  { to: '/products', key: 'landing.navProducts' },
  { to: '/pricing',  key: 'landing.navPricing'  },
  { to: '/docs',     key: 'landing.navDocs'      },
  { to: '/about',    key: 'landing.navAbout'     },
  { to: '/blog',     key: 'landing.navBlog'      },
]

export default function Navbar({ onSignIn }) {
  const { t, i18n } = useTranslation()
  const { pathname } = useLocation()

  function toggleLang() {
    i18n.changeLanguage(i18n.language === 'en' ? 'tr' : 'en')
  }

  return (
    <nav className="fixed top-0 left-0 right-0 z-50 flex items-center justify-between px-6 py-4 border-b border-white/5 bg-zinc-950/75 backdrop-blur-md">
      <Link to="/" className="font-thin tracking-[0.25em] text-sm select-none text-zinc-100 hover:text-white transition-colors">
        ASTRAEUSIO
      </Link>

      <div className="hidden md:flex items-center gap-10">
        {NAV_LINKS.map(({ to, key }) => (
          <Link
            key={to}
            to={to}
            className={`text-sm transition-colors ${
              pathname === to ? 'text-zinc-100' : 'text-zinc-400 hover:text-zinc-100'
            }`}
          >
            {t(key)}
          </Link>
        ))}
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
          className="text-zinc-300 hover:text-zinc-100 text-sm font-mono tracking-wide transition-colors"
        >
          {t('landing.nav')}
        </button>
      </div>
    </nav>
  )
}
