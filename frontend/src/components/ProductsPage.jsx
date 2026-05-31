import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import Navbar from './Navbar'
import Footer from './Footer'

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

const IconResearch = () => (
  <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="11" cy="11" r="7" />
    <line x1="21" y1="21" x2="16.65" y2="16.65" />
  </svg>
)
const IconSat = () => (
  <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M5 9l4-4 5 5-4 4z" />
    <path d="M11 13l4 4 4-4-4-5z" />
    <path d="M2 22l5-5M15 9l3-3" />
  </svg>
)
const IconAurora = () => (
  <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M3 18c2-6 5-9 9-9s7 3 9 9" />
    <path d="M5 18c1-3 3-5 7-5s6 2 7 5" />
  </svg>
)
const IconGrid = () => (
  <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
  </svg>
)

const AUDIENCE = [
  { key: 'aud1', Icon: IconResearch, cls: 'text-blue-400'   },
  { key: 'aud2', Icon: IconSat,      cls: 'text-orange-400' },
  { key: 'aud3', Icon: IconAurora,   cls: 'text-purple-400' },
  { key: 'aud4', Icon: IconGrid,     cls: 'text-emerald-400' },
]

const API_TABS = [
  {
    id: 'curl',
    label: 'curl',
    code: `curl -H "Authorization: Bearer ak_live_..." \\
  https://astraeusio.com/api/kp`,
  },
  {
    id: 'js',
    label: 'JavaScript',
    code: `const res = await fetch('https://astraeusio.com/api/kp', {
  headers: { Authorization: 'Bearer ak_live_...' }
})
const data = await res.json()`,
  },
  {
    id: 'python',
    label: 'Python',
    code: `import requests
r = requests.get(
  'https://astraeusio.com/api/kp',
  headers={'Authorization': 'Bearer ak_live_...'},
)
data = r.json()`,
  },
]

const API_RESPONSE = `{
  "time_tag":     "2026-05-31T15:00:00Z",
  "kp_index":     2,
  "estimated_kp": 1.83
}`

const PICK_KEYS = ['d1', 'd2', 'd3']

const PRODUCTS = [
  {
    key: 'd1',
    Icon: IconDashboard,
    accent: 'from-blue-500/20 to-blue-500/0',
    border: 'border-blue-500/30',
    iconBg: 'bg-blue-500/10',
    iconCls: 'text-blue-400',
    live: true,
    popular: false,
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
    live: true,
    popular: false,
    action: 'signup',
  },
]

export default function ProductsPage({ onSignIn, onSignUp }) {
  const { t } = useTranslation()
  const [apiTab, setApiTab] = useState('curl')
  const activeSample = API_TABS.find(s => s.id === apiTab) ?? API_TABS[0]

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">

      <Navbar onSignIn={onSignIn} />

      {/* ── Hero ─────────────────────────────────────────────────────────── */}
      <section className="pt-36 pb-16 px-6 text-center">
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

      {/* ── Which one do I pick ──────────────────────────────────────────── */}
      <section className="px-6 pt-6 pb-4 max-w-5xl mx-auto">
        <div className="flex flex-col gap-2 mb-6">
          <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase">
            {t('products.pickEyebrow')}
          </p>
          <h2 className="text-2xl md:text-3xl font-thin tracking-tight text-zinc-100">
            {t('products.pickTitle')}
          </h2>
        </div>
        <div className="flex flex-col divide-y divide-zinc-800 border-y border-zinc-800">
          {PICK_KEYS.map(k => (
            <div key={k} className="py-5 flex flex-col sm:flex-row sm:items-baseline gap-2 sm:gap-4">
              <p className="text-sm font-mono uppercase tracking-widest text-zinc-300 shrink-0 sm:w-40">
                {t(`products.pick.${k}.label`)}
              </p>
              <p className="text-zinc-400 text-sm leading-relaxed flex-1">
                {t(`products.pick.${k}.body`)}
              </p>
            </div>
          ))}
        </div>
      </section>

      {/* ── Audience ─────────────────────────────────────────────────────── */}
      <section className="px-6 pt-16 pb-20 max-w-5xl mx-auto">
        <div className="flex flex-col gap-2 mb-10">
          <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase">
            {t('products.audienceEyebrow')}
          </p>
          <h2 className="text-2xl md:text-3xl font-thin tracking-tight text-zinc-100">
            {t('products.audienceTitle')}
          </h2>
        </div>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          {AUDIENCE.map(({ key, Icon, cls }) => (
            <div key={key} className="bg-zinc-900 border border-zinc-800 rounded-xl p-5 flex flex-col gap-3">
              <div className={`w-9 h-9 rounded-lg bg-zinc-800/50 flex items-center justify-center ${cls}`}>
                <Icon />
              </div>
              <h3 className="text-sm font-semibold text-zinc-100">{t(`products.${key}Role`)}</h3>
              <p className="text-zinc-400 text-sm leading-relaxed">{t(`products.${key}Desc`)}</p>
            </div>
          ))}
        </div>
      </section>

      {/* ── API code block ───────────────────────────────────────────────── */}
      <section className="px-6 pb-20 max-w-5xl mx-auto">
        <div className="grid grid-cols-1 lg:grid-cols-5 gap-8 items-center">
          <div className="lg:col-span-2 flex flex-col gap-4">
            <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase">
              {t('products.apiEyebrow')}
            </p>
            <h2 className="text-2xl md:text-3xl font-thin tracking-tight text-zinc-100">
              {t('products.apiTitle')}
            </h2>
            <p className="text-zinc-400 text-sm leading-relaxed">
              {t('products.apiSub')}
            </p>
            <Link
              to="/docs"
              className="self-start text-sm text-orange-400 hover:text-orange-300 transition-colors"
            >
              {t('products.apiCtaDocs')} →
            </Link>
          </div>

          <div className="lg:col-span-3">
            <div className="bg-zinc-950 border border-zinc-800 rounded-xl overflow-hidden">
              <div className="flex items-center justify-between px-4 py-2.5 border-b border-zinc-800 bg-zinc-900/50">
                <div className="flex gap-1.5">
                  <span className="w-2.5 h-2.5 rounded-full bg-zinc-700" />
                  <span className="w-2.5 h-2.5 rounded-full bg-zinc-700" />
                  <span className="w-2.5 h-2.5 rounded-full bg-zinc-700" />
                </div>
                <span className="text-xs font-mono text-zinc-500">{t('products.apiCodeCaption')}</span>
              </div>

              {/* Language tabs */}
              <div className="flex border-b border-zinc-800 bg-zinc-900/30">
                {API_TABS.map(tab => {
                  const active = tab.id === apiTab
                  return (
                    <button
                      key={tab.id}
                      onClick={() => setApiTab(tab.id)}
                      className={`px-4 py-2 text-xs font-mono transition-colors border-b-2 -mb-px ${
                        active
                          ? 'border-orange-400 text-zinc-100'
                          : 'border-transparent text-zinc-500 hover:text-zinc-300'
                      }`}
                    >
                      {tab.label}
                    </button>
                  )
                })}
              </div>

              <pre className="text-xs font-mono text-zinc-300 p-4 overflow-x-auto leading-relaxed">
                <code>{activeSample.code}</code>
              </pre>

              <div className="px-4 py-2 border-y border-zinc-800 bg-zinc-900/40">
                <span className="text-[10px] font-mono uppercase tracking-widest text-zinc-500">
                  {t('products.apiResponseCaption')}
                </span>
              </div>
              <pre className="text-xs font-mono text-emerald-300/90 p-4 overflow-x-auto leading-relaxed">
                <code>{API_RESPONSE}</code>
              </pre>
            </div>
          </div>
        </div>
      </section>

      {/* ── Pricing nudge ────────────────────────────────────────────────── */}
      <section className="px-6 pb-10 text-center">
        <Link to="/pricing" className="text-zinc-500 hover:text-zinc-300 text-sm transition-colors">
          {t('products.seePricing')}
        </Link>
      </section>

      {/* ── Bottom CTA ───────────────────────────────────────────────────── */}
      <section className="px-6 pb-24 max-w-3xl mx-auto text-center">
        <div className="border border-zinc-800 rounded-2xl bg-gradient-to-b from-zinc-900 to-zinc-950 p-10 flex flex-col items-center gap-5">
          <h2 className="text-2xl md:text-3xl font-thin tracking-tight text-zinc-100 max-w-xl">
            {t('products.ctaTitle')}
          </h2>
          <p className="text-zinc-400 text-sm max-w-md leading-relaxed">
            {t('products.ctaSub')}
          </p>
          <div className="flex flex-col sm:flex-row items-center gap-3 mt-2">
            <button
              onClick={onSignUp}
              className="px-7 py-2.5 rounded-lg text-sm font-medium bg-zinc-100 text-zinc-950 hover:bg-white transition-colors"
            >
              {t('products.ctaPrimary')}
            </button>
            <Link
              to="/pricing"
              className="px-7 py-2.5 rounded-lg text-sm font-medium border border-zinc-700 text-zinc-300 hover:border-zinc-500 hover:text-zinc-100 transition-colors"
            >
              {t('products.ctaSecondary')}
            </Link>
          </div>
        </div>
      </section>

      <Footer />
    </div>
  )
}
