import { useState, useEffect, useCallback, useRef } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import * as THREE from 'three'
import { stormInfo, fmtNum } from '../lib/utils'
import KpChart       from './KpChart'
import ForecastPanel from './ForecastPanel'

// ── Live data hook ────────────────────────────────────────────────────────────

function useLiveWeather() {
  const [kpData,          setKpData]          = useState(null)
  const [wind,            setWind]            = useState(null)
  const [forecastData,    setForecastData]    = useState(null)
  const [forecastLoading, setForecastLoading] = useState(true)
  const [forecastError,   setForecastError]   = useState(null)
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
      if (f?.predicted_kp != null) {
        setForecastData(f)
        setForecastError(null)
      } else {
        setForecastData(null)
        setForecastError('Forecast unavailable')
      }
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

  return { kpData, wind, forecastData, forecastLoading, forecastError, age }
}

// ── Animated star field ───────────────────────────────────────────────────────

function HeroScene() {
  const containerRef = useRef(null)

  useEffect(() => {
    const el = containerRef.current
    if (!el) return

    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches

    // ── Renderer ──────────────────────────────────────────────────────────────
    const renderer = new THREE.WebGLRenderer({ antialias: true })
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    renderer.setSize(el.clientWidth, el.clientHeight)
    renderer.setClearColor(0x09090b, 1)   // zinc-950
    el.appendChild(renderer.domElement)

    // ── Scene & camera ────────────────────────────────────────────────────────
    const scene  = new THREE.Scene()
    const camera = new THREE.PerspectiveCamera(50, el.clientWidth / el.clientHeight, 0.1, 100)
    camera.position.set(0, 0.8, 4.2)
    camera.lookAt(0, 0, 0)

    // ── Lighting ──────────────────────────────────────────────────────────────
    scene.add(new THREE.AmbientLight(0x1a3a6a, 0.8))
    const sun = new THREE.DirectionalLight(0x6699cc, 1.5)
    sun.position.set(5, 3, 4)
    scene.add(sun)

    // ── Starfield ─────────────────────────────────────────────────────────────
    const STAR_COUNT = 2000
    const starPos    = new Float32Array(STAR_COUNT * 3)
    for (let i = 0; i < STAR_COUNT * 3; i++) starPos[i] = (Math.random() - 0.5) * 80
    const starGeo = new THREE.BufferGeometry()
    starGeo.setAttribute('position', new THREE.BufferAttribute(starPos, 3))
    scene.add(new THREE.Points(starGeo, new THREE.PointsMaterial({ color: 0xffffff, size: 0.07, sizeAttenuation: true })))

    // ── World group (earth + satellites offset from hero-text area) ───────────
    const world = new THREE.Group()
    world.position.set(0, 0, 0)
    scene.add(world)

    // ── Earth ─────────────────────────────────────────────────────────────────
    const earth = new THREE.Mesh(
      new THREE.SphereGeometry(1, 64, 64),
      new THREE.MeshPhongMaterial({
        color:             0x0d1f3c,
        emissive:          new THREE.Color(0x071428),
        emissiveIntensity: 0.7,
        shininess:         12,
        specular:          new THREE.Color(0x224488),
      }),
    )
    world.add(earth)

    // Atmosphere glow (two shells, BackSide — visible from outside)
    ;[{ r: 1.05, c: 0x2255aa, o: 0.12 }, { r: 1.14, c: 0x1133cc, o: 0.05 }].forEach(({ r, c, o }) => {
      world.add(new THREE.Mesh(
        new THREE.SphereGeometry(r, 32, 32),
        new THREE.MeshPhongMaterial({ color: c, transparent: true, opacity: o, side: THREE.BackSide }),
      ))
    })

    // ── Satellites ────────────────────────────────────────────────────────────
    const satGeo   = new THREE.SphereGeometry(0.022, 6, 6)
    const satMat   = new THREE.MeshBasicMaterial({ color: 0xddeeff })
    const lineMat  = new THREE.LineBasicMaterial({ color: 0x1a3060, transparent: true, opacity: 0.18 })

    function orbitLineGeo(r) {
      const pts = []
      for (let i = 0; i <= 64; i++) {
        const t = (i / 64) * Math.PI * 2
        pts.push(new THREE.Vector3(r * Math.cos(t), 0, r * Math.sin(t)))
      }
      return new THREE.BufferGeometry().setFromPoints(pts)
    }

    const sats = Array.from({ length: 20 }, () => {
      const radius = 1.55 + Math.random() * 0.9            // 1.55 – 2.45 earth radii
      const incl   = Math.random() * Math.PI * 0.85        // 0 – ~153° inclination
      const raan   = Math.random() * Math.PI * 2           // 0 – 360° ascending node
      const speed  = (0.004 + Math.random() * 0.008) * (Math.random() < 0.5 ? 1 : -1)
      const angle  = Math.random() * Math.PI * 2

      // Pivot sets the orbital plane orientation; children orbit in its local XZ plane
      const pivot = new THREE.Object3D()
      pivot.rotation.x = incl
      pivot.rotation.y = raan
      world.add(pivot)

      pivot.add(new THREE.LineLoop(orbitLineGeo(radius), lineMat))

      const sat = new THREE.Mesh(satGeo, satMat)
      sat.position.set(radius * Math.cos(angle), 0, radius * Math.sin(angle))
      pivot.add(sat)

      return { sat, radius, speed, angle }
    })

    // ── Resize ────────────────────────────────────────────────────────────────
    function onResize() {
      camera.aspect = el.clientWidth / el.clientHeight
      camera.updateProjectionMatrix()
      renderer.setSize(el.clientWidth, el.clientHeight)
    }
    window.addEventListener('resize', onResize)

    // ── Animate (skipped on prefers-reduced-motion) ───────────────────────────
    let raf
    if (reduced) {
      renderer.render(scene, camera)   // single static frame
    } else {
      function animate() {
        raf = requestAnimationFrame(animate)
        earth.rotation.y += 0.0008
        sats.forEach(s => {
          s.angle += s.speed
          s.sat.position.x = s.radius * Math.cos(s.angle)
          s.sat.position.z = s.radius * Math.sin(s.angle)
        })
        renderer.render(scene, camera)
      }
      animate()
    }

    // ── Cleanup ───────────────────────────────────────────────────────────────
    return () => {
      cancelAnimationFrame(raf)
      window.removeEventListener('resize', onResize)
      scene.traverse(obj => {
        if (obj.geometry) obj.geometry.dispose()
        if (obj.material) {
          const mats = Array.isArray(obj.material) ? obj.material : [obj.material]
          mats.forEach(m => m.dispose())
        }
      })
      renderer.dispose()
      if (el.contains(renderer.domElement)) el.removeChild(renderer.domElement)
    }
  }, [])

  return <div ref={containerRef} className="absolute inset-0" />
}

// ── Kp badge helper ───────────────────────────────────────────────────────────

function kpBadge(kp) {
  if (kp >= 7) return { text: 'text-red-400',     border: 'border-red-800/60',     dot: 'bg-red-400'     }
  if (kp >= 5) return { text: 'text-orange-400',  border: 'border-orange-800/60',  dot: 'bg-orange-400'  }
  if (kp >= 4) return { text: 'text-yellow-400',  border: 'border-yellow-800/60',  dot: 'bg-yellow-400'  }
  return              { text: 'text-emerald-400', border: 'border-emerald-900/60', dot: 'bg-emerald-400' }
}

// ── Landing-specific large metric card ────────────────────────────────────────

function LiveMetric({ label, value, unit, color = 'text-zinc-100', sub }) {
  return (
    <div className="bg-zinc-950 border border-zinc-800 rounded-2xl p-7 flex flex-col gap-3">
      <span className="text-xs font-mono tracking-[0.25em] text-zinc-500 uppercase">{label}</span>
      <div className="flex items-baseline gap-2.5 mt-1">
        <span className={`text-5xl font-bold tabular-nums leading-none ${color}`}>
          {value ?? '—'}
        </span>
        {unit && <span className="text-sm font-mono text-zinc-500">{unit}</span>}
      </div>
      {sub && <span className={`text-xs font-mono mt-1 ${color} opacity-75`}>{sub}</span>}
    </div>
  )
}

// ── Section label ─────────────────────────────────────────────────────────────

function SectionLabel({ children }) {
  return (
    <span className="text-xs font-mono tracking-[0.3em] text-zinc-500 uppercase">
      {children}
    </span>
  )
}

// ── SVG icons (capability cards) ──────────────────────────────────────────────

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

// ── Audience icons (24 px) ────────────────────────────────────────────────────

const IconEye = () => (
  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
    <circle cx="12" cy="12" r="3" />
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

const IconDish = () => (
  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 2a10 10 0 0 1 7.07 17.07" />
    <path d="M12 2a10 10 0 0 0-7.07 17.07" />
    <circle cx="12" cy="12" r="3" />
    <line x1="12" y1="15" x2="12" y2="22" />
    <line x1="9"  y1="22" x2="15" y2="22" />
  </svg>
)

const IconCode = () => (
  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="16 18 22 12 16 6" />
    <polyline points="8 6 2 12 8 18"   />
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

// ── Constants ─────────────────────────────────────────────────────────────────

const CAP_CONFIG = [
  { key: 'c1', Icon: IconStorm,     topBorder: 'border-t-orange-500', iconBg: 'bg-orange-500/10', iconCls: 'text-orange-400' },
  { key: 'c2', Icon: IconAurora,    topBorder: 'border-t-blue-500',   iconBg: 'bg-blue-500/10',   iconCls: 'text-blue-400'   },
  { key: 'c3', Icon: IconSatellite, topBorder: 'border-t-purple-500', iconBg: 'bg-purple-500/10', iconCls: 'text-purple-400' },
  { key: 'c4', Icon: IconAnomaly,   topBorder: 'border-t-red-500',    iconBg: 'bg-red-500/10',    iconCls: 'text-red-400'    },
]

const PRED_ITEMS = ['p1', 'p2', 'p3']
const SOURCES    = ['s1', 's2', 's3', 's4', 's5']

const AUDIENCE_CONFIG = [
  { key: 'a1', Icon: IconEye,    iconBg: 'bg-sky-500/10',     iconCls: 'text-sky-400'     },
  { key: 'a2', Icon: IconChart,  iconBg: 'bg-violet-500/10',  iconCls: 'text-violet-400'  },
  { key: 'a3', Icon: IconDish,   iconBg: 'bg-amber-500/10',   iconCls: 'text-amber-400'   },
  { key: 'a4', Icon: IconCode,   iconBg: 'bg-emerald-500/10', iconCls: 'text-emerald-400' },
  { key: 'a5', Icon: IconServer, iconBg: 'bg-rose-500/10',    iconCls: 'text-rose-400'    },
]

const TECH_SPECS = [
  ['MODEL',       'PyTorch LSTM'],
  ['TRAINING',    '20+ years NOAA Kp data'],
  ['UNCERTAINTY', 'Monte Carlo dropout · 50 passes · 95% CI'],
  ['REFRESH',     '1-minute ingestion cycles'],
  ['DETECTION',   'Multi-signal anomaly pipeline'],
]

// ── Page ──────────────────────────────────────────────────────────────────────

export default function LandingPage({ onSignUp, onSignIn }) {
  const { t, i18n } = useTranslation()
  const { kpData, wind, forecastData, forecastLoading, forecastError, age } = useLiveWeather()

  const latestKp = kpData?.at(-1)?.estimated_kp ?? null
  const storm    = stormInfo(latestKp ?? 0)
  const badge    = latestKp != null ? kpBadge(latestKp) : null

  function toggleLang() {
    i18n.changeLanguage(i18n.language === 'en' ? 'tr' : 'en')
  }

  return (
    <div className="bg-zinc-950 text-zinc-100">

      {/* ── Nav (fixed) ──────────────────────────────────────────────────── */}
      <nav className="fixed top-0 left-0 right-0 z-50 flex items-center justify-between px-6 py-4 border-b border-white/5 bg-zinc-950/75 backdrop-blur-md">
        <Link to="/" className="font-thin tracking-[0.25em] text-sm select-none text-zinc-100 hover:text-white transition-colors">
          ASTRAEUSIO
        </Link>
        <div className="hidden md:flex items-center gap-10">
          <Link to="/products" className="text-zinc-400 hover:text-zinc-100 text-sm transition-colors">{t('landing.navProducts')}</Link>
          <a href="/pricing"   className="text-zinc-400 hover:text-zinc-100 text-sm transition-colors">{t('landing.navPricing')}</a>
          <a href="/docs"      className="text-zinc-400 hover:text-zinc-100 text-sm transition-colors">{t('landing.navDocs')}</a>
          <a href="/about"     className="text-zinc-400 hover:text-zinc-100 text-sm transition-colors">{t('landing.navAbout')}</a>
          <a href="/blog"      className="text-zinc-400 hover:text-zinc-100 text-sm transition-colors">{t('landing.navBlog')}</a>
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

      {/* ── Hero ─────────────────────────────────────────────────────────── */}
      <section className="relative min-h-screen flex flex-col items-center justify-center text-center px-6 overflow-hidden">
        <HeroScene />

        {/* Bottom fade — blends into next section */}
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

          {/* Live Kp badge */}
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

        {/* Scroll indicator */}
        <div className="absolute bottom-10 left-1/2 -translate-x-1/2 z-10 flex flex-col items-center gap-1.5 text-zinc-700 animate-bounce">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="m6 9 6 6 6-6" />
          </svg>
        </div>
      </section>

      {/* ── Live Dashboard Preview (bg-zinc-900) ─────────────────────────── */}
      <section className="bg-zinc-900 py-20 px-6">
        <div className="max-w-5xl mx-auto">
          <div className="flex items-start justify-between mb-8">
            <div className="flex flex-col gap-2">
              <SectionLabel>{t('landing.liveTitle')}</SectionLabel>
              <p className="text-zinc-500 text-sm font-mono mt-1 max-w-md">{t('landing.liveDesc')}</p>
            </div>
            {age != null && (
              <span className="text-zinc-600 text-xs font-mono shrink-0 mt-0.5">
                {age === 0 ? t('landing.liveJustNow') : t('landing.liveUpdated', { s: age })}
              </span>
            )}
          </div>

          {/* Large metric cards */}
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 mb-4">
            <LiveMetric
              label={t('metrics.kpIndex')}
              value={latestKp != null ? fmtNum(latestKp, 2) : null}
              color={storm.cls}
              sub={t(storm.key)}
            />
            <LiveMetric
              label={t('metrics.solarWindSpeed')}
              value={wind?.speed != null ? fmtNum(wind.speed, 0) : null}
              unit="km/s"
              color={wind?.speed != null && wind.speed > 700 ? 'text-orange-400' : 'text-zinc-100'}
            />
            <LiveMetric
              label={t('metrics.protonDensity')}
              value={wind?.density != null ? fmtNum(wind.density, 1) : null}
              unit="p/cm³"
            />
          </div>

          {/* Chart + forecast */}
          <div className="grid grid-cols-3 gap-4">
            <div className="col-span-2">
              <KpChart records={kpData} />
            </div>
            <ForecastPanel
              data={forecastData}
              loading={forecastLoading}
              error={forecastError}
            />
          </div>
        </div>
      </section>

      {/* ── What You Can Do (bg-zinc-950) ────────────────────────────────── */}
      <section className="bg-zinc-950 py-20 px-6">
        <div className="max-w-5xl mx-auto">
          <div className="flex flex-col gap-2 mb-10">
            <SectionLabel>{t('landing.capTitle')}</SectionLabel>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-5">
            {CAP_CONFIG.map(({ key, Icon, topBorder, iconBg, iconCls }) => (
              <div
                key={key}
                className={`bg-zinc-900 rounded-xl p-7 flex flex-col gap-5 border border-zinc-800 border-t-2 ${topBorder}`}
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

      {/* ── Predictive Intelligence (bg-zinc-900) ────────────────────────── */}
      <section className="bg-zinc-900 py-20 px-6">
        <div className="max-w-5xl mx-auto">
          <SectionLabel>{t('landing.predTitle')}</SectionLabel>
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-8 mt-10">
            {PRED_ITEMS.map((pk, i) => (
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

      {/* ── How It Works + Data Sources (bg-zinc-950) ────────────────────── */}
      <section className="bg-zinc-950 py-20 px-6">
        <div className="max-w-5xl mx-auto grid grid-cols-1 sm:grid-cols-2 gap-14">
          <div className="flex flex-col gap-5">
            <SectionLabel>{t('landing.howTitle')}</SectionLabel>
            <p className="text-zinc-400 text-sm leading-relaxed">{t('landing.howBody')}</p>
            <p className="text-zinc-400 text-sm leading-relaxed">{t('landing.howMl')}</p>
          </div>
          <div className="flex flex-col gap-5">
            <SectionLabel>{t('landing.sourcesTitle')}</SectionLabel>
            <p className="text-zinc-600 text-xs font-mono">{t('landing.sourcesNote')}</p>
            <ul className="flex flex-col gap-3">
              {SOURCES.map(sk => (
                <li key={sk} className="flex items-start gap-3 text-sm text-zinc-400">
                  <span className="w-1.5 h-1.5 rounded-full bg-zinc-600 shrink-0 mt-1.5" />
                  {t(`landing.${sk}`)}
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
          {/* 3 + 2 centered grid */}
          <div className="grid grid-cols-1 sm:grid-cols-6 gap-5 mt-10">
            {AUDIENCE_CONFIG.map(({ key, Icon, iconBg, iconCls }, i) => (
              <div
                key={key}
                className={`sm:col-span-2${i === 3 ? ' sm:col-start-2' : ''}`}
              >
                <div className="bg-zinc-950 border border-zinc-800 rounded-xl p-6 flex flex-col gap-4 h-full">
                  <div className={`w-11 h-11 rounded-xl ${iconBg} flex items-center justify-center ${iconCls} shrink-0`}>
                    <Icon />
                  </div>
                  <div className="flex flex-col gap-1.5">
                    <h3 className="text-zinc-100 text-sm font-semibold">
                      {t(`landing.${key}`)}
                    </h3>
                    <p className="text-zinc-500 text-xs leading-relaxed">
                      {t(`landing.${key}desc`)}
                    </p>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* ── Technical Overview (bg-zinc-950, minimal, near bottom) ──────── */}
      <section className="bg-zinc-950 py-16 px-6">
        <div className="max-w-5xl mx-auto">
          <SectionLabel>{t('landing.techTitle')}</SectionLabel>
          <div className="mt-6 divide-y divide-zinc-900">
            {TECH_SPECS.map(([label, value]) => (
              <div key={label} className="flex items-baseline gap-6 py-3.5">
                <span className="text-xs font-mono text-zinc-600 tracking-[0.12em] uppercase w-28 shrink-0">
                  {label}
                </span>
                <span className="text-sm font-mono text-zinc-400">{value}</span>
              </div>
            ))}
          </div>
          <p className="text-zinc-600 text-xs font-mono mt-6">{t('landing.techNote')}</p>
        </div>
      </section>

      {/* ── CTA (full-width light section) ───────────────────────────────── */}
      <section className="bg-zinc-100 px-6 py-28 flex flex-col items-center text-center gap-6">
        <h2 className="text-4xl sm:text-5xl font-bold tracking-tight text-zinc-950 max-w-2xl leading-tight">
          {t('landing.ctaTitle')}
        </h2>
        <p className="text-zinc-600 text-lg max-w-lg leading-relaxed">
          {t('landing.ctaSub')}
        </p>
        <p className="text-zinc-500 text-sm">{t('landing.ctaBody')}</p>
        <button
          onClick={onSignUp}
          className="px-10 py-3.5 bg-zinc-950 text-zinc-100 text-sm font-mono tracking-widest uppercase rounded-lg hover:bg-zinc-800 transition-colors mt-2"
        >
          {t('landing.ctaBtn')}
        </button>
      </section>

      {/* ── Footer ───────────────────────────────────────────────────────── */}
      <footer className="bg-zinc-950 border-t border-zinc-900 px-6 py-5">
        <p className="text-zinc-600 text-xs font-mono text-center">
          {t('landing.footerNote')}
        </p>
      </footer>

    </div>
  )
}
