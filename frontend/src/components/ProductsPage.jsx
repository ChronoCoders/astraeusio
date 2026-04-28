import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import Navbar from './Navbar'

const IconDashboard = () => (
  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <rect x="3" y="3" width="7" height="7" rx="1" />
    <rect x="14" y="3" width="7" height="7" rx="1" />
    <rect x="3" y="14" width="7" height="7" rx="1" />
    <rect x="14" y="14" width="7" height="7" rx="1" />
  </svg>
)

const IconApi = () => (
  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="16 18 22 12 16 6" />
    <polyline points="8 6 2 12 8 18" />
  </svg>
)

const IconAlerts = () => (
  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
    <path d="M13.73 21a2 2 0 0 1-3.46 0" />
  </svg>
)

const PRODUCTS = [
  {
    key: 'd1',
    Icon: IconDashboard,
    accent: 'from-blue-500/20 to-blue-500/0',
    border: 'border-blue-500/30',
    iconBg: 'bg-blue-500/10',
    iconCls: 'text-blue-400',
    live: true,
    popular: true,
    action: 'signup',
  },
  {
    key: 'd2',
    Icon: IconApi,
    accent: 'from-orange-500/20 to-orange-500/0',
    border: 'border-orange-500/30',
    iconBg: 'bg-orange-500/10',
    iconCls: 'text-orange-400',
    live: true,
    popular: false,
    action: 'signup',
  },
  {
    key: 'd3',
    Icon: IconAlerts,
    accent: 'from-purple-500/20 to-purple-500/0',
    border: 'border-purple-500/30',
    iconBg: 'bg-purple-500/10',
    iconCls: 'text-purple-400',
    live: false,
    popular: false,
    action: 'notify',
  },
]

export default function ProductsPage({ onSignIn, onSignUp }) {
  const { t } = useTranslation()

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">

      <Navbar onSignIn={onSignIn} />

      {/* ── Hero ─────────────────────────────────────────────────────────── */}
      <section className="pt-40 pb-20 px-6 text-center">
        <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase mb-4">
          {t('products.heroEyebrow')}
        </p>
        <h1 className="text-4xl md:text-5xl font-thin tracking-tight text-zinc-100 max-w-2xl mx-auto mb-5">
          {t('products.heroTitle')}
        </h1>
        <p className="text-zinc-400 text-base max-w-xl mx-auto leading-relaxed">
          {t('products.heroSub')}
        </p>
      </section>

      {/* ── Product cards ─────────────────────────────────────────────────── */}
      <section className="px-6 pb-10 max-w-5xl mx-auto">
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          {PRODUCTS.map(({ key, Icon, accent, border, iconBg, iconCls, live, popular, action }) => (
            <div
              key={key}
              className={`relative rounded-2xl border ${border} bg-zinc-900 overflow-hidden flex flex-col`}
            >
              {/* gradient top wash */}
              <div className={`absolute inset-x-0 top-0 h-32 bg-gradient-to-b ${accent} pointer-events-none`} />

              <div className="relative z-10 p-8 flex flex-col gap-5 flex-1">
                {/* icon + status */}
                <div className="flex items-start justify-between">
                  <div className={`w-14 h-14 rounded-xl ${iconBg} flex items-center justify-center ${iconCls}`}>
                    <Icon />
                  </div>
                  <div className="flex flex-col items-end gap-1.5">
                    {popular && (
                      <span className="text-xs font-mono px-2.5 py-1 rounded-full bg-orange-500/10 border border-orange-500/30 text-orange-400">
                        {t('products.mostPopular')}
                      </span>
                    )}
                    <span className={`text-xs font-mono px-2.5 py-1 rounded-full border ${
                      live
                        ? 'text-green-400 border-green-500/30 bg-green-500/10'
                        : 'text-zinc-500 border-zinc-700 bg-zinc-800'
                    }`}>
                      {live ? t('products.live') : t('products.comingSoon')}
                    </span>
                  </div>
                </div>

                {/* text */}
                <div className="flex flex-col gap-1.5">
                  <h2 className="text-xl font-semibold text-zinc-100">{t(`products.${key}Title`)}</h2>
                  <p className="text-xs font-mono text-zinc-500 uppercase tracking-widest">{t(`products.${key}Sub`)}</p>
                </div>

                <p className="text-zinc-400 text-sm leading-relaxed flex-1">
                  {t(`products.${key}Desc`)}
                </p>

                {/* CTA */}
                <div className="mt-auto">
                  {action === 'signup' ? (
                    <button
                      onClick={onSignUp}
                      className="w-full py-2.5 rounded-lg text-sm font-medium transition-colors bg-zinc-100 text-zinc-950 hover:bg-white"
                    >
                      {t(`products.${key}Cta`)}
                    </button>
                  ) : action === 'notify' ? (
                    <button
                      onClick={onSignUp}
                      className="w-full py-2.5 rounded-lg text-sm font-medium transition-colors border border-zinc-600 hover:border-zinc-400 text-zinc-300 hover:text-zinc-100 bg-transparent"
                    >
                      {t(`products.${key}Cta`)}
                    </button>
                  ) : (
                    <button
                      className="w-full py-2.5 rounded-lg text-sm font-medium bg-zinc-800 text-zinc-500 cursor-not-allowed"
                      disabled
                    >
                      {t(`products.${key}Cta`)}
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}
        </div>
      </section>

      {/* ── Pricing nudge ────────────────────────────────────────────────── */}
      <section className="px-6 pb-20 text-center">
        <Link to="/pricing" className="text-zinc-500 hover:text-zinc-300 text-sm transition-colors">
          {t('products.seePricing')}
        </Link>
      </section>

      {/* ── Footer ───────────────────────────────────────────────────────── */}
      <footer className="border-t border-zinc-800 px-6 py-10 text-center">
        <Link to="/" className="text-zinc-500 hover:text-zinc-300 text-sm transition-colors">
          {t('products.backHome')}
        </Link>
        <p className="text-zinc-700 text-xs mt-4">{t('landing.footerNote')}</p>
      </footer>

    </div>
  )
}
