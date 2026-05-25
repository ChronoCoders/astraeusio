import { useState, useEffect, useRef, useCallback, lazy, Suspense } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { stormInfo, fmtNum } from '../lib/utils'
import Navbar from './Navbar'
import Footer from './Footer'

const HeroScene = lazy(() => import('./HeroScene'))

// ── Scroll fade hook ─────────────────────────────────────────────────────────

function useFadeIn(delay = 0) {
  const ref = useRef(null)
  const [visible, setVisible] = useState(false)
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const obs = new IntersectionObserver(
      ([entry]) => { if (entry.isIntersecting) { setVisible(true); obs.disconnect() } },
      { threshold: 0.08 }
    )
    obs.observe(el)
    return () => obs.disconnect()
  }, [])
  return [ref, {
    opacity:    visible ? 1 : 0,
    transform:  visible ? 'none' : 'translateY(28px)',
    transition: `opacity 0.65s ease ${delay}ms, transform 0.65s ease ${delay}ms`,
  }]
}

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

  return { kpData, wind, forecastData, forecastLoading, age, fetchedAt }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function kpBadge(kp) {
  if (kp >= 7) return { text: 'text-red-400',     border: 'border-red-800/60',     dot: 'bg-red-400'     }
  if (kp >= 5) return { text: 'text-orange-400',  border: 'border-orange-800/60',  dot: 'bg-orange-400'  }
  if (kp >= 4) return { text: 'text-yellow-400',  border: 'border-yellow-800/60',  dot: 'bg-yellow-400'  }
  return              { text: 'text-emerald-400', border: 'border-emerald-900/60', dot: 'bg-emerald-400' }
}

function SectionLabel({ children, as: Tag = 'span' }) {
  return (
    <Tag className="text-xs font-mono tracking-[0.3em] text-zinc-500 uppercase">
      {children}
    </Tag>
  )
}

// Live Kp sparkline — reuses the /api/public/kp series already fetched above.
function KpSparkline({ data }) {
  const { t } = useTranslation()
  const pts = (data || []).filter(r => r.estimated_kp > 0)
  if (pts.length < 2) return null

  const series = pts.slice(-180)            // ~last 3 h at 1-min cadence
  const W = 900, H = 84, PAD = 8, MAX = 9
  const n = series.length
  const xx = i  => PAD + (i / (n - 1)) * (W - 2 * PAD)
  const yy = kp => H - PAD - (Math.min(Math.max(kp, 0), MAX) / MAX) * (H - 2 * PAD)
  const line = series.map((r, i) => `${i === 0 ? 'M' : 'L'} ${xx(i).toFixed(1)} ${yy(r.estimated_kp).toFixed(1)}`).join(' ')
  const area = `${line} L ${xx(n - 1).toFixed(1)} ${(H - PAD).toFixed(1)} L ${xx(0).toFixed(1)} ${(H - PAD).toFixed(1)} Z`
  const last = series[n - 1]
  const badge = kpBadge(last.estimated_kp)

  return (
    <div className="mt-4 bg-zinc-900/60 border border-zinc-800 rounded-lg p-4">
      <div className="flex items-center justify-between mb-3">
        <span className="text-xs font-mono tracking-[0.2em] text-zinc-500 uppercase">{t('landing.sparkTitle')}</span>
        <span className={`text-xs font-mono ${badge.text}`}>Kp {fmtNum(last.estimated_kp, 2)}</span>
      </div>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="w-full" style={{ height: 84 }} aria-hidden="true">
        <defs>
          <linearGradient id="kpSpark" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%"   stopColor="#fb923c" stopOpacity="0.28" />
            <stop offset="100%" stopColor="#fb923c" stopOpacity="0" />
          </linearGradient>
        </defs>
        {/* G1 storm threshold (Kp 5) */}
        <line x1={PAD} y1={yy(5)} x2={W - PAD} y2={yy(5)} stroke="#a16207" strokeWidth="1" strokeDasharray="5 4" opacity="0.7" vectorEffect="non-scaling-stroke" />
        <path d={area} fill="url(#kpSpark)" />
        <path d={line} fill="none" stroke="#fb923c" strokeWidth="1.6" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />
      </svg>
      <div className="flex items-center justify-between mt-2">
        <span className="text-zinc-600 text-[10px] font-mono uppercase tracking-widest">{t('landing.sparkSpan')}</span>
        <span className="text-zinc-600 text-[10px] font-mono">{t('landing.sparkStorm')}</span>
      </div>
    </div>
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
  const { kpData, wind, forecastData, forecastLoading, age, fetchedAt } = useLiveWeather()
  const [techOpen, setTechOpen] = useState(false)
  const [flash,    setFlash]    = useState(false)

  useEffect(() => {
    if (!fetchedAt) return
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setFlash(true)
    const t = setTimeout(() => setFlash(false), 600)
    return () => clearTimeout(t)
  }, [fetchedAt])

  const latestKp = kpData?.filter(r => r.estimated_kp > 0)?.at(-1)?.estimated_kp ?? null
  const storm    = stormInfo(latestKp ?? 0)
  const badge    = latestKp != null ? kpBadge(latestKp) : null

  const [capRef,      capStyle]      = useFadeIn(0)
  const [liveRef,     liveStyle]     = useFadeIn(0)
  const [predRef,     predStyle]     = useFadeIn(0)
  const [howRef,      howStyle]      = useFadeIn(0)
  const [audienceRef, audienceStyle] = useFadeIn(0)
  const [techRef,     techStyle]     = useFadeIn(0)
  const [ctaRef,      ctaStyle]      = useFadeIn(0)

  return (
    <div className="bg-zinc-950 text-zinc-100">

      <Navbar onSignIn={onSignIn} />

      {/* ── Hero ─────────────────────────────────────────────────────────── */}
      <section className="relative min-h-screen px-6 overflow-hidden">
        <div className="absolute bottom-0 left-0 right-0 h-40 bg-gradient-to-t from-zinc-900 to-transparent pointer-events-none z-10" />

        <div className="relative max-w-7xl mx-auto min-h-screen grid grid-cols-1 lg:grid-cols-2 gap-10 lg:gap-12 items-center pt-28 lg:pt-24 pb-20">

          {/* Left: text */}
          <div className="flex flex-col items-start gap-7 text-left max-w-xl">
            <SectionLabel>{t('landing.heroEyebrow')}</SectionLabel>

            <h1 className="text-5xl sm:text-6xl lg:text-7xl font-bold tracking-tight leading-[1.08]">
              <span className="bg-gradient-to-b from-white via-zinc-100 to-zinc-400 bg-clip-text text-transparent">
                {t('landing.heroTitle')}
              </span>
            </h1>

            <p className="text-zinc-400 text-lg sm:text-xl leading-relaxed">
              {t('landing.heroSub')}
            </p>

            <div className="border-l-2 border-zinc-600 pl-4 w-full">
              <p className="text-zinc-300 text-sm font-mono leading-relaxed">
                {t('landing.heroStat')}
              </p>
            </div>

            {badge && (
              <div className={`inline-flex items-center gap-2.5 px-4 py-2 rounded-full border ${badge.border} bg-zinc-900/50 backdrop-blur-sm`}>
                <span className="relative shrink-0 w-2 h-2">
                  <span className={`absolute inset-0 rounded-full ${badge.dot} animate-pulse`} />
                  {flash && <span className={`absolute -inset-1 rounded-full ${badge.dot} opacity-40 animate-ping`} />}
                </span>
                <span className={`text-sm font-mono ${badge.text}`}>
                  Live · Kp {fmtNum(latestKp, 1)} · {t(storm.key)}
                </span>
              </div>
            )}

            <div className="flex flex-col sm:flex-row items-start gap-3 mt-1">
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

          {/* Right: Earth scene */}
          <div className="relative w-full aspect-square max-w-md sm:max-w-lg mx-auto lg:max-w-none lg:mx-0 lg:ml-auto lg:h-[600px] lg:aspect-auto lg:w-[600px]">
            <Suspense fallback={null}>
              <HeroScene />
            </Suspense>
          </div>

        </div>
      </section>

      {/* ── What You Can Do (bg-zinc-900) ────────────────────────────────── */}
      <section ref={capRef} style={capStyle} className="bg-zinc-900 py-20 px-6">
        <div className="max-w-5xl mx-auto">
          <div className="flex flex-col gap-2 mb-10">
            <SectionLabel as="h2">{t('landing.capTitle')}</SectionLabel>
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
          <div className="mt-8 flex items-center justify-between">
            <Link to="/about" className="text-xs font-mono text-zinc-500 hover:text-zinc-300 transition-colors tracking-wide">
              {t('landing.aboutUs')} →
            </Link>
            <Link to="/products" className="text-xs font-mono text-zinc-500 hover:text-zinc-300 transition-colors tracking-wide">
              {t('landing.allFeatures')} →
            </Link>
          </div>
        </div>
      </section>

      {/* ── Live Right Now (bg-zinc-950) ──────────────────────────────────── */}
      <section ref={liveRef} style={liveStyle} className="bg-zinc-950 py-20 px-6">
        <div className="max-w-5xl mx-auto">
          <div className="flex items-end justify-between mb-8">
            <SectionLabel as="h2">{t('landing.liveTitle')}</SectionLabel>
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

          <KpSparkline data={kpData} />
        </div>
      </section>

      {/* ── Predictive Intelligence (bg-zinc-900) ────────────────────────── */}
      <section ref={predRef} style={predStyle} className="bg-zinc-900 py-20 px-6">
        <div className="max-w-5xl mx-auto">
          <SectionLabel as="h2">{t('landing.predTitle')}</SectionLabel>
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
      <section ref={howRef} style={howStyle} className="bg-zinc-950 py-20 px-6">
        <div className="max-w-5xl mx-auto grid grid-cols-1 sm:grid-cols-2 gap-14">
          <div className="flex flex-col gap-5">
            <SectionLabel as="h2">{t('landing.howTitle')}</SectionLabel>
            <p className="text-zinc-400 text-sm leading-relaxed">
              {t('landing.howBody')}
            </p>
            <p className="text-zinc-400 text-sm leading-relaxed">
              {t('landing.howMl')}
            </p>
          </div>
          <div className="flex flex-col gap-5">
            <SectionLabel as="h2">{t('landing.sourcesTitle')}</SectionLabel>
            <p className="text-zinc-600 text-xs font-mono">{t('landing.sourcesNote')}</p>
            <ul className="flex flex-col gap-3">
              {SOURCE_KEYS.map(k => (
                <li key={k} className="flex items-start gap-3 text-sm text-zinc-400">
                  <span className="w-1.5 h-1.5 rounded-full bg-zinc-600 shrink-0 mt-1.5" />
                  {t(`landing.${k}`)}
                </li>
              ))}
            </ul>
            <div className="flex flex-col gap-2 self-start">
              <Link to="/docs" className="text-xs font-mono text-zinc-500 hover:text-zinc-300 transition-colors tracking-wide">
                {t('landing.apiDocs')} →
              </Link>
              <Link to="/blog" className="text-xs font-mono text-zinc-500 hover:text-zinc-300 transition-colors tracking-wide">
                {t('landing.blog')} →
              </Link>
              <Link to="/status" className="text-xs font-mono text-zinc-500 hover:text-zinc-300 transition-colors tracking-wide">
                {t('landing.status')} →
              </Link>
            </div>
          </div>
        </div>
      </section>

      {/* ── Who It's For (bg-zinc-900) ───────────────────────────────────── */}
      <section ref={audienceRef} style={audienceStyle} className="bg-zinc-900 py-20 px-6">
        <div className="max-w-5xl mx-auto">
          <SectionLabel as="h2">{t('landing.audienceTitle')}</SectionLabel>
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
      <section ref={techRef} style={techStyle} className="bg-zinc-950 py-14 px-6 border-t border-zinc-900">
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
      <section ref={ctaRef} style={ctaStyle} className="bg-zinc-100 px-6 py-28 flex flex-col items-center text-center gap-6">
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
        <Link to="/pricing" className="text-sm text-zinc-500 hover:text-zinc-700 transition-colors font-mono">
          {t('landing.comparePlans')} →
        </Link>
      </section>

      {/* ── SEO text block — visually hidden, crawler-visible ───────────── */}
      <div aria-hidden="true" style={{position:'absolute',width:'1px',height:'1px',overflow:'hidden',clip:'rect(0,0,0,0)',whiteSpace:'nowrap',border:0}}>

        <h2>About Astraeusio</h2>
        <p>
          Astraeusio is a real-time space weather monitoring and prediction platform. It aggregates
          data from NOAA, NASA, and the Kyoto World Data Center to deliver live geomagnetic storm
          tracking, solar wind analysis, X-ray flux monitoring, and ML-powered Kp forecasts from a
          single dashboard. The platform is designed for satellite operators, HF radio operators,
          aurora chasers, power grid engineers, researchers, and developers who need reliable,
          continuously updated space weather intelligence.
        </p>

        <h2>Dashboard Product</h2>
        <p>
          The Astraeusio dashboard provides real-time monitoring of all major space weather parameters.
          It displays the Kp geomagnetic index updated every 60 seconds, solar wind speed and proton
          density from NOAA DSCOVR, X-ray flux from GOES satellites, the interplanetary magnetic field
          Bz component, the Dst storm-time disturbance index, NOAA space weather alerts, aurora oval
          forecast images, ISS orbital position, NASA Astronomy Picture of the Day, NASA EPIC Earth
          imagery, near-Earth asteroid tracking, Starlink constellation orbital data, and exoplanet
          catalog data. The dashboard runs continuous anomaly detection every 60 seconds across five
          event types and surfaces detected events in a dedicated alerts panel.
        </p>

        <h2>API Product</h2>
        <p>
          The Astraeusio REST API provides programmatic access to all space weather data streams.
          Public endpoints require no authentication and return the latest Kp index, solar wind
          readings, and ML Kp forecast. Authenticated endpoints, accessed via API key or JWT token,
          cover the full Kp time series, 3-hour official Kp, solar wind history, X-ray flux, IMF
          magnetometer data, Dst index, NOAA space weather alerts, anomaly detection results,
          near-Earth object close approaches, ISS position, NASA APOD, NASA EPIC imagery, exoplanet
          catalog, Starlink TLE data, and summary reports with CSV export. The API follows REST
          conventions with JSON responses and uses Bearer token authentication. Rate limits and
          available endpoints vary by pricing plan.
        </p>

        <h2>Alerts Product</h2>
        <p>
          The Astraeusio alerts system runs automated anomaly detection every 60 seconds. It checks
          five conditions: geomagnetic storm onset when Kp reaches 5 or above, high-speed solar wind
          when speed exceeds 700 km/s, solar flares when X-ray flux reaches M-class or X-class levels,
          near-Earth asteroid close approaches within one lunar distance, and ML forecast storm
          warnings when the model predicts Kp 5 or above within 3 hours. All detected events are
          logged with severity levels and timestamps. Users can receive notifications via email alerts
          with configurable Kp and solar wind thresholds, or via webhook for automated integration
          into monitoring pipelines and operations tools.
        </p>

        <h2>ML-Powered Storm Prediction</h2>
        <p>
          The machine learning forecast system uses an LSTM neural network trained on over 20 years
          of historical NOAA Kp index data. The model ingests a 7-feature input vector including
          normalized Kp, hour of day encoded as sine and cosine, month of year encoded as sine and
          cosine, and solar cycle phase. During inference, Monte Carlo Dropout sampling runs 50
          forward passes to produce a probabilistic output: a predicted Kp value for 3 hours ahead,
          a lower confidence bound, an upper confidence bound at 95%, and an uncertainty estimate.
          The model was trained with early stopping and validated on a held-out test set. Forecasts
          are cached for 3 minutes and refreshed on demand via the API.
        </p>

        <h2>Kp Geomagnetic Index</h2>
        <p>
          The planetary Kp index measures global geomagnetic activity on a scale from 0 to 9. Values
          below 2 indicate quiet conditions with no significant aurora activity. Kp 3 to 4 is
          unsettled, with minor disturbances possible at high latitudes. Kp 5 marks the onset of a
          G1 minor geomagnetic storm, when auroras become visible above 60 degrees latitude and
          minor power grid fluctuations may occur. Kp 6 is G2 moderate, with possible long-duration
          power system effects and auroras above 55 degrees. Kp 7 is G3 strong, with voltage
          corrections needed on power systems and possible GPS degradation. Kp 8 is G4 severe with
          widespread power control issues. Kp 9 is G5 extreme, the highest level, with potential
          for complete HF radio blackout and major grid instability. Astraeusio tracks both the
          1-minute estimated Kp from real-time magnetometer data and the official 3-hour planetary
          Kp index published by NOAA.
        </p>

        <h2>Solar Wind and Interplanetary Magnetic Field</h2>
        <p>
          Solar wind measurements come from NOAA DSCOVR, a satellite positioned at the L1 Lagrange
          point approximately 1.5 million kilometers from Earth. DSCOVR measures solar wind speed in
          km/s, proton density in particles per cubic centimeter, and the interplanetary magnetic
          field vector. The southward component of the IMF, known as Bz, is a critical storm driver:
          sustained negative Bz allows solar wind energy to couple into Earth's magnetosphere,
          intensifying geomagnetic storm conditions. The platform also tracks the Dst disturbance
          storm-time index from the Kyoto World Data Center, which measures the global suppression
          of Earth's horizontal magnetic field caused by ring current intensification during storms.
        </p>

        <h2>X-Ray Flux and Solar Flares</h2>
        <p>
          X-ray flux data comes from NOAA GOES geostationary satellites. Solar flares are classified
          by peak X-ray flux: A and B class flares are minor with no significant effects. C-class
          flares may cause weak radio interference. M-class flares, with flux above 10 to the minus
          5 watts per square meter, can cause HF radio blackouts on the sunlit side of Earth.
          X-class flares, with flux above 10 to the minus 4 watts per square meter, are the most
          powerful and can cause widespread radio blackouts, radiation storms, and are often
          associated with coronal mass ejections that drive geomagnetic storms one to three days later.
        </p>

        <h2>Data Sources</h2>
        <p>
          Astraeusio ingests data from the following sources: NOAA Space Weather Prediction Center
          for Kp index, solar wind, X-ray flux, and space weather alert text products; NASA NeoWs
          API for near-Earth asteroid close approach data updated every 30 minutes; NASA APOD for
          the daily astronomy picture; NASA EPIC for full-disk Earth imagery from DSCOVR; NASA
          Exoplanet Archive for confirmed exoplanet catalog data; Open Notify for ISS orbital
          position updated every 5 seconds; Celestrak for Starlink TLE orbital elements updated
          every hour; and the Kyoto World Data Center for the Dst geomagnetic disturbance index.
          All data is fetched with retry logic, cached at the backend layer, and persisted in a
          DuckDB database for historical queries and report generation.
        </p>

        <h2>Who Uses Astraeusio</h2>
        <p>
          Satellite operators use Astraeusio to monitor geomagnetic storm risk and receive advance
          warnings before conditions intensify enough to affect orbital operations or onboard
          electronics. HF radio operators and aviators on polar routes rely on X-ray flux and Kp
          data to anticipate radio blackout conditions and plan frequency changes. Aurora chasers
          use the Kp index and aurora oval forecast images to determine visibility likelihood at
          their latitude and plan observation sessions. Power grid and pipeline operators monitor
          Kp and Dst to anticipate geomagnetically induced currents that can affect infrastructure.
          Researchers and data scientists access the API and CSV export features to build datasets
          for analysis and modeling. Developers integrate the REST API into applications, monitoring
          dashboards, and automated alert systems using webhook delivery.
        </p>

        <h2>Pricing Plans</h2>
        <p>
          Astraeusio offers five pricing tiers. The Free plan provides 100 API requests per day
          with a 60-second data delay and access to Kp and solar wind data. The Developer plan
          provides 10,000 requests per month with real-time data, ML Kp forecast with confidence
          intervals, basic anomaly detection, and limited email alerts. The Pro plan provides
          100,000 requests per month, full anomaly detection, webhook alerts, and priority support.
          The Business plan provides 1,000,000 requests per month, advanced alerting with custom
          thresholds, multi-channel notifications, and SLA-backed uptime. The Enterprise plan
          provides unlimited requests with dedicated infrastructure, custom anomaly models, and
          dedicated support. All plans start with a free tier requiring no credit card.
        </p>

        <h2>API Endpoints</h2>
        <p>
          Public API endpoints available without authentication: GET /api/public/kp returns the
          latest Kp reading; GET /api/public/solar-wind returns the latest solar wind speed and
          proton density; GET /api/public/forecast returns the latest ML Kp forecast with
          confidence interval. Authenticated endpoints require a Bearer token: GET /api/kp returns
          the full 1-minute Kp time series; GET /api/kp-3h returns the official 3-hour Kp index;
          GET /api/kp-forecast returns the cached ML forecast; GET /api/solar-wind returns the
          full solar wind history; GET /api/xray returns X-ray flux readings; GET /api/imf returns
          IMF magnetometer data; GET /api/dst returns the Dst index; GET /api/alerts returns NOAA
          space weather alert text; GET /api/anomalies returns detected anomaly events;
          GET /api/neo returns near-Earth asteroid close approaches for the next 7 days;
          GET /api/iss returns current ISS position; GET /api/apod returns the NASA astronomy
          picture; GET /api/epic returns NASA EPIC Earth imagery; GET /api/starlink returns
          Starlink TLE data; GET /api/exoplanets returns the exoplanet catalog;
          GET /api/reports/summary with a range parameter of 24h, 7d, or 30d returns aggregated
          statistics; GET /api/reports/export returns a CSV file of readings for the selected period.
          Authentication endpoints: POST /auth/register creates a new account; POST /auth/login
          returns a JWT token valid for 24 hours.
        </p>

        <p>
          <a href="/products">View all products</a> ·{' '}
          <a href="/pricing">Compare pricing plans</a> ·{' '}
          <a href="/docs">API documentation</a> ·{' '}
          <a href="/about">About Astraeusio</a> ·{' '}
          <a href="/blog">Space weather blog</a> ·{' '}
          <a href="/status">Platform status</a>
        </p>
      </div>

      <Footer />

    </div>
  )
}
