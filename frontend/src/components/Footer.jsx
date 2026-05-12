import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'

function GitHubIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z" />
    </svg>
  )
}

function Col({ heading, children }) {
  return (
    <div className="flex flex-col gap-3">
      <span className="text-zinc-400 text-[10px] uppercase tracking-widest font-mono">{heading}</span>
      {children}
    </div>
  )
}

function NavLink({ to, children }) {
  return (
    <Link to={to} className="text-zinc-500 hover:text-zinc-200 text-xs transition-colors w-fit">
      {children}
    </Link>
  )
}

export default function Footer() {
  const { t } = useTranslation()

  return (
    <footer className="bg-zinc-950 border-t border-zinc-800/60 mt-auto">

      {/* ── Main grid ─────────────────────────────────────────────────────── */}
      <div className="max-w-6xl mx-auto px-6 py-14 grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-10">

        {/* Wordmark column */}
        <div className="flex flex-col gap-4">
          <Link to="/" className="font-thin tracking-[0.25em] text-sm text-zinc-100 hover:text-white transition-colors w-fit select-none">
            ASTRAEUSIO
          </Link>
          <p className="text-zinc-500 text-xs leading-relaxed max-w-[180px]">
            {t('footer.tagline')}
          </p>
          <a
            href="https://github.com/ChronoCoders/astraeusio"
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-1.5 text-zinc-500 hover:text-zinc-300 text-xs font-mono transition-colors w-fit"
          >
            <GitHubIcon />
            GitHub
          </a>
        </div>

        {/* Products */}
        <Col heading={t('footer.products')}>
          <NavLink to="/products">{t('landing.navProducts')}</NavLink>
          <NavLink to="/pricing">{t('landing.navPricing')}</NavLink>
        </Col>

        {/* Resources */}
        <Col heading={t('footer.resources')}>
          <NavLink to="/docs">{t('landing.navDocs')}</NavLink>
          <NavLink to="/blog">{t('landing.navBlog')}</NavLink>
        </Col>

        {/* Company */}
        <Col heading={t('footer.company')}>
          <NavLink to="/about">{t('landing.navAbout')}</NavLink>
          <NavLink to="/status">{t('landing.navStatus')}</NavLink>
          <a
            href="mailto:contact@chronocoder.dev"
            className="text-zinc-500 hover:text-zinc-200 text-xs transition-colors w-fit"
          >
            {t('footer.contact')}
          </a>
        </Col>
      </div>

      {/* ── Bottom bar ────────────────────────────────────────────────────── */}
      <div className="border-t border-zinc-800/60">
        <div className="max-w-6xl mx-auto px-6 py-4">
          <p className="text-zinc-600 text-[11px] font-mono">{t('footer.bottomBar')}</p>
        </div>
      </div>

    </footer>
  )
}
