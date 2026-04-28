import { useState, useEffect, useCallback, lazy, Suspense } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { stormInfo, fmtNum } from '../lib/utils'
import Navbar from './Navbar'

const HeroScene = lazy(() => import('./HeroScene'))

// ── Live data hook ────────────────────────────────────────────────────────────

function useLiveWeather() {
  const [kpData,          setKpData]          = useState(null)
  const [wind,            setWind]            = useState(null)
  const [forecastData,    setForecastData]    = useState(null)
  const [forecastLoading, setForecastLoading] = useState(true)
  const [fetchedAt,       setFetchedAt]       = useState(null)
  const [age,             setAge]             = useState(null)

  const fetch3 = useCallback(() => {
    const get = url => fetch(url).then(r => r.ok ? r.json() : null).catch(() => null)
    Promise.all([
      get('/api/public/kp'),
      get('/api/public/solar-wind'),
      get('/api/public/forecast'),
    ]).then(([k, w, f]) => {
      setKpData(Array.isArray(k) ? k : null)
      setWind(w)
      setForecastData(f?.predicted_kp != null ? f : null)
      setForecastLoading(false)
      setFetchedAt(Date.now())
      setAge(0)
    })
  }, [])

  useEffect(() => {
    const id = setInterval(fetch3, 60_000)
    const t  = setTimeout(fetch3, 0)
    return () => { clearInterval(id); clearTimeout(t) }
  }, [fetch3])

  useEffect(() => {
    if (fetchedAt == null) return
    const id = setInterval(() => setAge(Math.floor((Date.now() - fetchedAt) / 1000)), 1000)
    return () => clearInterval(id)
  }, [fetchedAt])

  return { kpData, wind, forecastData, forecastLoading, age }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function kpBadge(kp) {
  if (kp >= 7) return { text: 'text-red-400',     border: 'border-red-800/60',     dot: 'bg-red-400'     }
  if (kp >= 5) return { text: 'text-orange-400',  border: 'border-orange-800/60',  dot: 'bg-orange-400'  }
  if (kp >= 4) return { text: 'text-yellow-400',  border: 'border-yellow-800/60',  dot: 'bg-yellow-400'  }
  return              { text: 'text-emerald-400', border: 'border-emerald-900/60', dot: 'bg-emerald-400' }
}

function SectionLabel({ children }) {
  return (
    <span className="text-xs font-mono tracking-[0.3em] text-zinc-500 uppercase">
      {children}
    </span>
  )
}

// ── Live metric card ──────────────────────────────────────────────────────────

function LiveMetric({ label, value, unit, color = 'text-zinc-100', sub, delayed }) {
  const { t } = useTranslation()
  return (
    <div className="bg-zinc-950 border border-zinc-800 rounded-2xl p-7 flex flex-col gap-3">
      <span className="text-xs font-mono tracking-[0.25em] text-zinc-500 uppercase">{label}</span>
      <div className="flex items-baseline gap-2.5 mt-1">
        <span className={`text-5xl font-bold tabular-nums leading-none ${delayed ? 'text-zinc-600' : color}`}>
          {value ?? '—'}
        </span>
        {unit && !delayed && <span className="text-sm font-mono text-zinc-500">{unit}</span>}
      </div>
      {delayed
        ? <span className="text-xs font-mono text-zinc-600">{t('landing.liveDelayed')}</span>
        : sub && <span className={`text-xs font-mono mt-1 ${color} opacity-75`}>{sub}</span>
      }
    </div>
  )
}

// ── Icons ─────────────────────────────────────────────────────────────────────

const IconStorm = () => (
  <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="12" cy="12" r="4" />
    <line x1="12" y1="2"  x2="12" y2="5"  />
    <line x1="12" y1="19" x2="12" y2="22" />
    <line x1="4.22"  y1="4.22"  x2="6.34"  y2="6.34"  />
    <line x1="17.66" y1="17.66" x2="19.78" y2="19.78" />
    <line x1="2"  y1="12" x2="5"  y2="12" />
    <line x1="19" y1="12" x2="22" y2="12" />
    <line x1="4.22"  y1="19.78" x2="6.34"  y2="17.66" />
    <line x1="17.66" y1="6.34"  x2="19.78" y2="4.22"  />
  </svg>
)

const IconAurora = () => (
  <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M3 19 Q7 10 12 14 Q17 18 21 9" />
    <path d="M3 15 Q7  7 12 10 Q17 13 21 5" />
    <line x1="3" y1="22" x2="21" y2="22" />
  </svg>
)

const IconSatellite = () => (
  <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <ellipse cx="12" cy="12" rx="10" ry="4" transform="rotate(-35 12 12)" />
    <circle cx="12" cy="12" r="2.5" />
    <circle cx="19" cy="7" r="1" fill="currentColor" stroke="none" />
  </svg>
)

const IconAnomaly = () => (
  <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
    <line x1="12" y1="9"  x2="12" y2="13" />
    <line x1="12" y1="17" x2="12.01" y2="17" />
  </svg>
)

const IconDish = () => (
  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 2a10 10 0 0 1 7.07 17.07" />
    <path d="M12 2a10 10 0 0 0-7.07 17.07" />
    <circle cx="12" cy="12" r="3" />
    <line x1="12" y1="15" x2="12" y2="22" />
    <line x1="9"  y1="22" x2="15" y2="22" />
  </svg>
)

const IconServer = () => (
  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <rect x="2" y="2"  width="20" height="8" rx="2" />
    <rect x="2" y="14" width="20" height="8" rx="2" />
    <line x1="6" y1="6"  x2="6.01" y2="6"  />
    <line x1="6" y1="18" x2="6.01" y2="18" />
  </svg>
)

const IconCode = () => (
  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="16 18 22 12 16 6" />
    <polyline points="8 6 2 12 8 18"   />
  </svg>
)

const IconChart = () => (
  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <line x1="18" y1="20" x2="18" y2="10" />
    <line x1="12" y1="20" x2="12" y2="4"  />
    <line x1="6"  y1="20" x2="6"  y2="14" />
    <line x1="2"  y1="20" x2="22" y2="20" />
  </svg>
)

const IconEye = () => (
  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
    <circle cx="12" cy="12" r="3" />
  </svg>
)

// ── Data (keys only, no human-readable strings) ───────────────────────────────

const CAP_CONFIG = [
  { key: 'c1', Icon: IconStorm,     topBorder: 'border-t-orange-500', iconBg: 'bg-orange-500/10', iconCls: 'text-orange-400' },
  { key: 'c2', Icon: IconAurora,    topBorder: 'border-t-blue-500',   iconBg: 'bg-blue-500/10',   iconCls: 'text-blue-400'   },
  { key: 'c3', Icon: IconSatellite, topBorder: 'border-t-purple-500', iconBg: 'bg-purple-500/10', iconCls: 'text-purple-400' },
  { key: 'c4', Icon: IconAnomaly,   topBorder: 'border-t-red-500',    iconBg: 'bg-red-500/10',    iconCls: 'text-red-400'    },
]

// Operators first, enthusiasts last
const AUDIENCE_CONFIG = [
  { key: 'a3', Icon: IconDish,      iconBg: 'bg-amber-500/10',   iconCls: 'text-amber-400'   },
  { key: 'a5', Icon: IconServer,    iconBg: 'bg-rose-500/10',    iconCls: 'text-rose-400'    },
  { key: 'a4', Icon: IconCode,      iconBg: 'bg-emerald-500/10', iconCls: 'text-emerald-400' },
  { key: 'a2', Icon: IconChart,     iconBg: 'bg-violet-500/10',  iconCls: 'text-violet-400'  },
  { key: 'a1', Icon: IconEye,       iconBg: 'bg-sky-500/10',     iconCls: 'text-sky-400'     },
]

const SOURCE_KEYS = ['s1', 's2', 's3', 's4', 's5']

const TECH_SPECS = ['techModel', 'techTraining', 'techUncertainty', 'techRefresh', 'techDetection']

// ── Page ──────────────────────────────────────────────────────────────────────

export default function LandingPage({ onSignUp, onSignIn }) {
  const { t } = useTranslation()
  const { kpData, wind, forecastData, forecastLoading, age } = useLiveWeather()
  const [techOpen, setTechOpen] = useState(false)

  const latestKp = kpData?.filter(r => r.estimated_kp > 0)?.at(-1)?.estimated_kp ?? null
  const storm    = stormInfo(latestKp ?? 0)
  const badge    = latestKp != null ? kpBadge(latestKp) : null

  return (
    <div className="bg-zinc-950 text-zinc-100">

      <Navbar onSignIn={onSignIn} />

      {/* ── Hero ─────────────────────────────────────────────────────────── */}
      <section className="relative min-h-screen flex flex-col items-center justify-center text-center px-6 overflow-hidden">
        <Suspense fallback={null}>
          <HeroScene />
        </Suspense>

        <div className="absolute bottom-0 left-0 right-0 h-40 bg-gradient-to-t from-zinc-900 to-transparent pointer-events-none" />

        <div className="relative z-10 flex flex-col items-center gap-7 max-w-4xl pt-20">
          <SectionLabel>{t('landing.heroEyebrow')}</SectionLabel>

          <h1 className="text-5xl sm:text-6xl lg:text-7xl font-bold tracking-tight leading-[1.08]">
            <span className="bg-gradient-to-b from-white via-zinc-100 to-zinc-400 bg-clip-text text-transparent">
              {t('landing.heroTitle')}
            </span>
          </h1>

          <p className="text-zinc-400 text-lg sm:text-xl max-w-xl leading-relaxed">
            {t('landing.heroSub')}
          </p>

          <div className="border-l-2 border-zinc-600 pl-4 text-left w-full max-w-lg">
            <p className="text-zinc-300 text-sm font-mono leading-relaxed">
              {t('landing.heroStat')}
            </p>
          </div>

          {badge && (
            <div className={`inline-flex items-center gap-2.5 px-4 py-2 rounded-full border ${badge.border} bg-zinc-900/50 backdrop-blur-sm`}>
              <span className={`w-2 h-2 rounded-full ${badge.dot} animate-pulse shrink-0`} />
              <span className={`text-sm font-mono ${badge.text}`}>
                Live · Kp {fmtNum(latestKp, 1)} · {t(storm.key)}
              </span>
            </div>
          )}

          <div className="flex flex-col sm:flex-row items-center gap-3 mt-1">
            <button
              onClick={onSignUp}
              className="px-8 py-3 bg-zinc-100 text-zinc-950 text-sm font-mono tracking-wide rounded-lg hover:bg-white transition-colors"
            >
              {t('landing.ctaPrimary')}
            </button>
            <button
              onClick={onSignIn}
              className="px-8 py-3 border border-zinc-700 text-zinc-300 text-sm font-mono tracking-wide rounded-lg hover:border-zinc-500 hover:text-zinc-100 transition-colors"
            >
              {t('landing.ctaSecondary')}
            </button>
          </div>
        </div>

        <div className="absolute bottom-10 left-1/2 -translate-x-1/2 z-10 flex flex-col items-center gap-1.5 text-zinc-700 animate-bounce">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="m6 9 6 6 6-6" />
          </svg>
        </div>
      </section>

      {/* ── What You Can Do (bg-zinc-900) ────────────────────────────────── */}
      <section className="bg-zinc-900 py-20 px-6">
        <div className="max-w-5xl mx-auto">
          <div className="flex flex-col gap-2 mb-10">
            <SectionLabel>{t('landing.capTitle')}</SectionLabel>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-5">
            {CAP_CONFIG.map(({ key, Icon, topBorder, iconBg, iconCls }) => (
              <div
                key={key}
                className={`bg-zinc-950 rounded-xl p-7 flex flex-col gap-5 border border-zinc-800 border-t-2 ${topBorder}`}
              >
                <div className={`w-12 h-12 rounded-xl ${iconBg} flex items-center justify-center ${iconCls} shrink-0`}>
                  <Icon />
                </div>
                <div className="flex flex-col gap-2">
                  <h3 className="text-zinc-100 text-base font-semibold tracking-tight">
                    {t(`landing.${key}Title`)}
                  </h3>
                  <p className="text-zinc-400 text-sm leading-relaxed">
                    {t(`landing.${key}Body`)}
                  </p>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* ── Live Right Now (bg-zinc-950) ──────────────────────────────────── */}
      <section className="bg-zinc-950 py-20 px-6">
        <div className="max-w-5xl mx-auto">
          <div className="flex items-end justify-between mb-8">
            <SectionLabel>{t('landing.liveTitle')}</SectionLabel>
            {age != null && (
              <span className="text-zinc-600 text-xs font-mono">
                {age === 0
                  ? t('landing.liveJustNow')
                  : t('landing.liveUpdated', { s: age })}
              </span>
            )}
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
            <LiveMetric
              label={t('metrics.kpIndex')}
              value={latestKp != null ? fmtNum(latestKp, 2) : null}
              color={storm.cls}
              sub={t(storm.key)}
              delayed={kpData === null && !forecastLoading}
            />
            <LiveMetric
              label={t('metrics.solarWindSpeed')}
              value={wind?.speed != null ? fmtNum(wind.speed, 0) : null}
              unit="km/s"
              color={wind?.speed != null && wind.speed > 700 ? 'text-orange-400' : 'text-zinc-100'}
              delayed={wind === null && !forecastLoading}
            />
            <LiveMetric
              label={t('forecast.title')}
              value={forecastData != null ? fmtNum(forecastData.predicted_kp, 1) : null}
              sub={t('forecast.predictedKp')}
              color={forecastData != null ? stormInfo(forecastData.predicted_kp).cls : 'text-zinc-100'}
              delayed={forecastData === null && !forecastLoading}
            />
          </div>
        </div>
      </section>

      {/* ── Predictive Intelligence (bg-zinc-900) ────────────────────────── */}
      <section className="bg-zinc-900 py-20 px-6">
        <div className="max-w-5xl mx-auto">
          <SectionLabel>{t('landing.predTitle')}</SectionLabel>
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-8 mt-10">
            {['p1', 'p2', 'p3'].map((pk, i) => (
              <div key={pk} className="flex flex-col gap-3">
                <span className="text-xs font-mono text-zinc-700 select-none">
                  {String(i + 1).padStart(2, '0')}
                </span>
                <h3 className="text-zinc-100 text-base font-semibold tracking-tight">
                  {t(`landing.${pk}Title`)}
                </h3>
                <p className="text-zinc-400 text-sm leading-relaxed">
                  {t(`landing.${pk}Body`)}
                </p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* ── How It Works (bg-zinc-950) ────────────────────────────────────── */}
      <section className="bg-zinc-950 py-20 px-6">
        <div className="max-w-5xl mx-auto grid grid-cols-1 sm:grid-cols-2 gap-14">
          <div className="flex flex-col gap-5">
            <SectionLabel>{t('landing.howTitle')}</SectionLabel>
            <p className="text-zinc-400 text-sm leading-relaxed">
              {t('landing.howBody')}
            </p>
            <p className="text-zinc-400 text-sm leading-relaxed">
              {t('landing.howMl')}
            </p>
          </div>
          <div className="flex flex-col gap-5">
            <SectionLabel>{t('landing.sourcesTitle')}</SectionLabel>
            <p className="text-zinc-600 text-xs font-mono">{t('landing.sourcesNote')}</p>
            <ul className="flex flex-col gap-3">
              {SOURCE_KEYS.map(k => (
                <li key={k} className="flex items-start gap-3 text-sm text-zinc-400">
                  <span className="w-1.5 h-1.5 rounded-full bg-zinc-600 shrink-0 mt-1.5" />
                  {t(`landing.${k}`)}
                </li>
              ))}
            </ul>
          </div>
        </div>
      </section>

      {/* ── Who It's For (bg-zinc-900) ───────────────────────────────────── */}
      <section className="bg-zinc-900 py-20 px-6">
        <div className="max-w-5xl mx-auto">
          <SectionLabel>{t('landing.audienceTitle')}</SectionLabel>
          <div className="grid grid-cols-1 sm:grid-cols-6 gap-5 mt-10">
            {AUDIENCE_CONFIG.map(({ key, Icon, iconBg, iconCls }, i) => (
              <div key={key} className={`sm:col-span-2${i === 3 ? ' sm:col-start-2' : ''}`}>
                <div className="bg-zinc-950 border border-zinc-800 rounded-xl p-6 flex flex-col gap-4 h-full">
                  <div className={`w-11 h-11 rounded-xl ${iconBg} flex items-center justify-center ${iconCls} shrink-0`}>
                    <Icon />
                  </div>
                  <div className="flex flex-col gap-1.5">
                    <h3 className="text-zinc-100 text-sm font-semibold">{t(`landing.${key}`)}</h3>
                    <p className="text-zinc-500 text-xs leading-relaxed">{t(`landing.${key}desc`)}</p>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* ── Technical Overview (collapsed, bg-zinc-950) ───────────────────── */}
      <section className="bg-zinc-950 py-14 px-6 border-t border-zinc-900">
        <div className="max-w-5xl mx-auto">
          <button
            onClick={() => setTechOpen(o => !o)}
            className="flex items-center gap-3 group w-full text-left"
          >
            <SectionLabel>{t('landing.techTitle')}</SectionLabel>
            <svg
              width="14" height="14" viewBox="0 0 24 24" fill="none"
              stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"
              className={`text-zinc-600 group-hover:text-zinc-400 transition-transform ${techOpen ? 'rotate-180' : ''}`}
            >
              <path d="m6 9 6 6 6-6" />
            </svg>
          </button>

          {techOpen && (
            <div className="mt-6">
              <div className="divide-y divide-zinc-900">
                {TECH_SPECS.map(key => (
                  <div key={key} className="flex items-baseline gap-6 py-3.5">
                    <span className="text-xs font-mono text-zinc-600 tracking-[0.12em] uppercase w-28 shrink-0">
                      {t(`landing.${key}`)}
                    </span>
                    <span className="text-sm font-mono text-zinc-400">{t(`landing.${key}Val`)}</span>
                  </div>
                ))}
              </div>
              <p className="text-zinc-600 text-xs font-mono mt-6">
                {t('landing.techNote')}
              </p>
            </div>
          )}
        </div>
      </section>

      {/* ── CTA (full-width light) ────────────────────────────────────────── */}
      <section className="bg-zinc-100 px-6 py-28 flex flex-col items-center text-center gap-6">
        <h2 className="text-4xl sm:text-5xl font-bold tracking-tight text-zinc-950 max-w-2xl leading-tight">
          {t('landing.ctaTitle')}
        </h2>
        <p className="text-zinc-600 text-lg max-w-lg leading-relaxed">
          {t('landing.ctaSub')}
        </p>
        <button
          onClick={onSignUp}
          className="px-10 py-3.5 bg-zinc-950 text-zinc-100 text-sm font-mono tracking-widest uppercase rounded-lg hover:bg-zinc-800 transition-colors mt-2"
        >
          {t('landing.ctaBtn')}
        </button>
      </section>

      {/* ── Footer ───────────────────────────────────────────────────────── */}
      <footer className="bg-zinc-950 border-t border-zinc-900 px-6 py-10">
        <div className="max-w-5xl mx-auto flex flex-col items-center gap-6">
          <nav className="flex flex-wrap justify-center gap-x-8 gap-y-2">
            {[
              ['/products', t('landing.navProducts')],
              ['/pricing',  t('landing.navPricing')],
              ['/docs',     t('landing.navDocs')],
              ['/about',    t('landing.navAbout')],
              ['/blog',     t('landing.navBlog')],
            ].map(([to, label]) => (
              <Link key={to} to={to} className="text-zinc-500 hover:text-zinc-300 text-xs font-mono tracking-wide transition-colors">
                {label}
              </Link>
            ))}
          </nav>
          <p className="text-zinc-600 text-xs font-mono text-center">
            {t('landing.footerNote')}
          </p>
        </div>
      </footer>

    </div>
  )
}
