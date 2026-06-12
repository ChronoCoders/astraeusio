import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import Logo from './Logo'

function StatusBadge() {
  const { t } = useTranslation()
  const [status, setStatus] = useState(null)
  useEffect(() => {
    let cancelled = false
    const load = () => {
      fetch('/api/health')
        .then(r => r.ok ? r.json() : Promise.reject())
        .then(d => { if (!cancelled) setStatus(d.status) })
        .catch(() => { if (!cancelled) setStatus('outage') })
    }
    load()
    const id = setInterval(load, 60_000)
    return () => { cancelled = true; clearInterval(id) }
  }, [])
  const cfg = status === 'operational'
    ? { dot: 'bg-green-500', text: 'text-zinc-400', label: t('footer.statusOk') }
    : status === 'degraded'
    ? { dot: 'bg-yellow-500', text: 'text-zinc-400', label: t('footer.statusDegraded') }
    : status === 'outage'
    ? { dot: 'bg-red-500', text: 'text-zinc-400', label: t('footer.statusOutage') }
    : { dot: 'bg-zinc-600', text: 'text-zinc-500', label: t('footer.statusChecking') }
  return (
    <Link
      to="/status"
      className={`inline-flex items-center gap-2 text-[11px] font-mono ${cfg.text} hover:text-zinc-200 transition-colors`}
    >
      <span className="relative inline-flex w-2 h-2">
        <span className={`absolute inset-0 rounded-full ${cfg.dot}`} />
        {status === 'operational' && (
          <span className={`absolute inset-0 rounded-full ${cfg.dot} opacity-50 animate-ping`} />
        )}
      </span>
      {cfg.label}
    </Link>
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
          <Link
            to="/"
            onClick={() => window.scrollTo({ top: 0, behavior: 'smooth' })}
            className="flex items-center gap-2.5 w-fit text-zinc-100 hover:text-white transition-colors select-none"
          >
            <Logo size={28} className="shrink-0 -ml-1.5" />
            <span className="font-thin tracking-[0.25em] text-sm">ASTRAEUSIO</span>
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
        <div className="max-w-6xl mx-auto px-6 py-4 grid grid-cols-1 sm:grid-cols-3 items-center gap-3">
          <div className="flex justify-start">
            <StatusBadge />
          </div>
          <p className="text-zinc-600 text-[11px] font-mono sm:text-center">
            {t('footer.bottomBar')}
          </p>
          <div className="flex items-center gap-4 sm:justify-end">
            <NavLink to="/privacy">{t('footer.privacy')}</NavLink>
            <NavLink to="/terms">{t('footer.terms')}</NavLink>
          </div>
        </div>
      </div>

    </footer>
  )
}
