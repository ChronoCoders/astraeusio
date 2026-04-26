import { useTranslation } from 'react-i18next'

const FEATURES = [
  { tk: 'f1', icon: '☀' },
  { tk: 'f2', icon: '⚡' },
  { tk: 'f3', icon: '🛰' },
  { tk: 'f4', icon: '☄' },
  { tk: 'f5', icon: '🌌' },
  { tk: 'f6', icon: '🔔' },
]

const SOURCES = ['s1', 's2', 's3', 's4', 's5']

export default function LandingPage({ onSignUp, onSignIn }) {
  const { t, i18n } = useTranslation()

  function toggleLang() {
    i18n.changeLanguage(i18n.language === 'en' ? 'tr' : 'en')
  }

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 flex flex-col">

      {/* ── Nav ──────────────────────────────────────────────────────────── */}
      <nav className="flex items-center justify-between px-6 py-4 border-b border-zinc-900">
        <span className="text-zinc-100 font-thin tracking-[0.25em] text-sm select-none">
          ASTRAEUSIO
        </span>
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
      <section className="flex flex-col items-center text-center px-6 py-20 gap-6">
        <div className="flex flex-col items-center gap-2">
          <span className="text-xs font-mono tracking-[0.3em] text-zinc-500 uppercase">
            Space Weather Intelligence
          </span>
          <h1 className="text-4xl sm:text-5xl font-extralight tracking-tight text-zinc-100 max-w-2xl leading-tight">
            {t('landing.heroTitle')}
          </h1>
        </div>
        <p className="text-zinc-400 text-base max-w-xl leading-relaxed">
          {t('landing.heroSub')}
        </p>
        <div className="flex flex-col sm:flex-row items-center gap-3 mt-2">
          <button
            onClick={onSignUp}
            className="px-6 py-2.5 bg-zinc-100 text-zinc-950 text-sm font-mono tracking-wide rounded hover:bg-white transition-colors"
          >
            {t('landing.ctaPrimary')}
          </button>
          <button
            onClick={onSignIn}
            className="px-6 py-2.5 border border-zinc-700 text-zinc-300 text-sm font-mono tracking-wide rounded hover:border-zinc-500 hover:text-zinc-100 transition-colors"
          >
            {t('landing.ctaSecondary')}
          </button>
        </div>
      </section>

      {/* ── Features ─────────────────────────────────────────────────────── */}
      <section className="px-6 pb-16 max-w-5xl mx-auto w-full">
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {FEATURES.map(({ tk, icon }) => (
            <div
              key={tk}
              className="bg-zinc-900 border border-zinc-800 rounded p-5 flex flex-col gap-2"
            >
              <span className="text-2xl">{icon}</span>
              <h3 className="text-zinc-100 text-sm font-mono tracking-wide">
                {t(`landing.${tk}Title`)}
              </h3>
              <p className="text-zinc-400 text-xs leading-relaxed">
                {t(`landing.${tk}Body`)}
              </p>
            </div>
          ))}
        </div>
      </section>

      {/* ── Data sources ─────────────────────────────────────────────────── */}
      <section className="border-t border-zinc-900 px-6 py-12 max-w-5xl mx-auto w-full">
        <h2 className="text-xs font-mono tracking-[0.3em] text-zinc-500 uppercase mb-6">
          {t('landing.sourcesTitle')}
        </h2>
        <ul className="flex flex-col gap-2">
          {SOURCES.map(sk => (
            <li key={sk} className="flex items-center gap-2 text-xs text-zinc-400 font-mono">
              <span className="w-1.5 h-1.5 rounded-full bg-zinc-600 shrink-0" />
              {t(`landing.${sk}`)}
            </li>
          ))}
        </ul>
      </section>

      {/* ── ML callout ───────────────────────────────────────────────────── */}
      <section className="border-t border-zinc-900 px-6 py-12 max-w-5xl mx-auto w-full">
        <div className="bg-zinc-900 border border-zinc-800 rounded p-6 flex flex-col gap-3">
          <h2 className="text-sm font-mono tracking-wide text-zinc-100">
            {t('landing.mlTitle')}
          </h2>
          <p className="text-zinc-400 text-xs leading-relaxed max-w-2xl">
            {t('landing.mlBody')}
          </p>
          <div className="flex flex-wrap gap-2 mt-1">
            {['PyTorch LSTM', 'MC Dropout', '50 forward passes', '95% CI', '3h horizon'].map(tag => (
              <span
                key={tag}
                className="text-xs font-mono text-zinc-500 border border-zinc-800 rounded px-2 py-0.5"
              >
                {tag}
              </span>
            ))}
          </div>
        </div>
      </section>

      {/* ── Bottom CTA ───────────────────────────────────────────────────── */}
      <section className="border-t border-zinc-900 px-6 py-16 flex flex-col items-center text-center gap-4">
        <h2 className="text-2xl font-extralight tracking-tight text-zinc-100">
          {t('landing.ctaTitle')}
        </h2>
        <p className="text-zinc-400 text-sm">{t('landing.ctaSub')}</p>
        <button
          onClick={onSignUp}
          className="px-6 py-2.5 bg-zinc-100 text-zinc-950 text-sm font-mono tracking-wide rounded hover:bg-white transition-colors mt-2"
        >
          {t('landing.ctaBtn')}
        </button>
      </section>

      {/* ── Footer ───────────────────────────────────────────────────────── */}
      <footer className="border-t border-zinc-900 px-6 py-4 mt-auto">
        <p className="text-zinc-600 text-xs font-mono text-center">
          {t('landing.footerNote')}
        </p>
      </footer>

    </div>
  )
}
