import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'

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
          <Link
            to="/"
            onClick={() => window.scrollTo({ top: 0, behavior: 'smooth' })}
            className="font-thin tracking-[0.25em] text-sm text-zinc-100 hover:text-white transition-colors w-fit select-none"
          >
            ASTRAEUSIO
          </Link>
          <p className="text-zinc-500 text-xs leading-relaxed max-w-[180px]">
            {t('footer.tagline')}
          </p>
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
            href="mailto:hello@astraeusio.com"
            className="text-zinc-500 hover:text-zinc-200 text-xs transition-colors w-fit"
          >
            {t('footer.contact')}
          </a>
        </Col>
      </div>

      {/* ── Bottom bar ────────────────────────────────────────────────────── */}
      <div className="border-t border-zinc-800/60">
        <div className="max-w-6xl mx-auto px-6 py-4 flex flex-wrap items-center justify-between gap-3">
          <p className="text-zinc-600 text-[11px] font-mono">{t('footer.bottomBar')}</p>
          <div className="flex items-center gap-4">
            <NavLink to="/privacy">{t('footer.privacy')}</NavLink>
            <NavLink to="/terms">{t('footer.terms')}</NavLink>
          </div>
        </div>
      </div>

    </footer>
  )
}
