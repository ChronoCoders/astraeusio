import { useTranslation } from 'react-i18next'
import Navbar from './Navbar'
import Footer from './Footer'

function Section({ id, children, className = '' }) {
  return (
    <section id={id} className={`max-w-3xl mx-auto px-6 py-20 ${className}`}>
      {children}
    </section>
  )
}

function Label({ children }) {
  return (
    <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase mb-4">{children}</p>
  )
}

function Divider() {
  return <div className="border-t border-zinc-800 max-w-3xl mx-auto px-6" />
}

const LiveIcon = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="w-5 h-5">
    <path strokeLinecap="round" strokeLinejoin="round" d="M3.75 13.5l10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75z" />
  </svg>
)

const ConfidenceIcon = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="w-5 h-5">
    <path strokeLinecap="round" strokeLinejoin="round" d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z" />
  </svg>
)

const SpeedIcon = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="w-5 h-5">
    <path strokeLinecap="round" strokeLinejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
  </svg>
)

const FallbackIcon = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="w-5 h-5">
    <path strokeLinecap="round" strokeLinejoin="round" d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z" />
  </svg>
)

export default function AboutPage({ onSignIn }) {
  const { t } = useTranslation()

  const stats = [
    { value: t('about.stat1Value'), label: t('about.stat1Label') },
    { value: t('about.stat2Value'), label: t('about.stat2Label') },
    { value: t('about.stat3Value'), label: t('about.stat3Label') },
    { value: t('about.stat4Value'), label: t('about.stat4Label') },
  ]

  const builtCards = [
    { icon: <LiveIcon />,       title: t('about.built1Title'), desc: t('about.built1Desc') },
    { icon: <ConfidenceIcon />, title: t('about.built2Title'), desc: t('about.built2Desc') },
    { icon: <SpeedIcon />,      title: t('about.built3Title'), desc: t('about.built3Desc') },
    { icon: <FallbackIcon />,   title: t('about.built4Title'), desc: t('about.built4Desc') },
  ]

  const sources = [
    { name: 'NOAA Space Weather Prediction Center', url: 'https://www.swpc.noaa.gov',          desc: t('about.src1Desc') },
    { name: 'NASA APIs',                            url: 'https://api.nasa.gov',               desc: t('about.src2Desc') },
    { name: 'Celestrak',                            url: 'https://celestrak.org',              desc: t('about.src3Desc') },
    { name: 'Kyoto World Data Center for Geomagnetism', url: 'https://wdc.kugi.kyoto-u.ac.jp', desc: t('about.src4Desc') },
  ]

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <Navbar onSignIn={onSignIn} />

      {/* Hero */}
      <section className="max-w-3xl mx-auto px-6 pt-36 pb-16">
        <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase mb-4">{t('about.eyebrow')}</p>
        <h1 className="text-4xl md:text-5xl font-thin tracking-tight text-zinc-100 leading-tight">
          {t('about.heroTitle')}
        </h1>
      </section>

      {/* Stat row */}
      <div className="max-w-3xl mx-auto px-6 pb-20">
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-px bg-zinc-800 rounded-xl overflow-hidden">
          {stats.map(({ value, label }) => (
            <div key={label} className="bg-zinc-950 px-6 py-6 flex flex-col gap-1">
              <span className="text-3xl font-thin text-orange-400 tracking-tight">{value}</span>
              <span className="text-xs font-mono text-zinc-500 uppercase tracking-wide">{label}</span>
            </div>
          ))}
        </div>
      </div>

      <Divider />

      {/* Mission */}
      <Section id="mission">
        <Label>{t('about.missionLabel')}</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-6">{t('about.missionTitle')}</h2>
        <div className="space-y-4 text-zinc-400 text-sm leading-relaxed">
          <p>{t('about.missionP1')}</p>
          <p>{t('about.missionP2')}</p>
          <p>{t('about.missionP3')}</p>
        </div>
        <div className="mt-10 flex flex-col sm:flex-row items-start gap-5 border-l-2 border-orange-400/30 pl-5">
          <picture>
            <source srcSet="/founder-altug.webp" type="image/webp" />
            <img
              src="/founder-altug.jpg"
              alt="Altug Tatlisu"
              width="80"
              height="80"
              loading="lazy"
              className="w-20 h-20 rounded-full object-cover border border-zinc-800 shrink-0"
            />
          </picture>
          <p className="text-zinc-300 text-sm leading-relaxed italic">
            {t('about.founderNote')}
          </p>
        </div>
      </Section>

      <Divider />

      {/* How it's built - icon grid */}
      <Section id="built">
        <Label>{t('about.builtLabel')}</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-8">{t('about.builtTitle')}</h2>
        <div className="grid sm:grid-cols-2 gap-4">
          {builtCards.map(({ icon, title, desc }) => (
            <div key={title} className="border border-zinc-800 rounded-xl p-5 bg-zinc-900/30 flex gap-4">
              <div className="mt-0.5 text-orange-400 shrink-0">{icon}</div>
              <div>
                <p className="text-sm font-medium text-zinc-200 mb-1">{title}</p>
                <p className="text-zinc-400 text-sm leading-relaxed">{desc}</p>
              </div>
            </div>
          ))}
        </div>
      </Section>

      <Divider />

      {/* Architecture / data flow */}
      <Section id="architecture">
        <Label>{t('about.archLabel')}</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-3">{t('about.archTitle')}</h2>
        <p className="text-zinc-400 text-sm leading-relaxed mb-8">{t('about.archDesc')}</p>
        <div className="grid grid-cols-1 md:grid-cols-[1fr_auto_1fr_auto_1fr] gap-4 md:gap-3 items-stretch">
          {/* Ingest */}
          <div className="border border-zinc-800 rounded-xl p-5 bg-zinc-900/40">
            <p className="text-xs font-mono uppercase tracking-widest text-orange-400 mb-3">{t('about.archStep1')}</p>
            <ul className="flex flex-col gap-2 text-zinc-300 text-sm">
              <li>NOAA SWPC</li>
              <li>NASA APIs</li>
              <li>Kyoto WDC</li>
              <li>Celestrak</li>
            </ul>
          </div>
          <div className="hidden md:flex items-center justify-center text-zinc-600 text-2xl font-thin">→</div>
          {/* Process */}
          <div className="border border-zinc-800 rounded-xl p-5 bg-zinc-900/40">
            <p className="text-xs font-mono uppercase tracking-widest text-orange-400 mb-3">{t('about.archStep2')}</p>
            <ul className="flex flex-col gap-2 text-zinc-300 text-sm">
              <li>{t('about.archProc1')}</li>
              <li>{t('about.archProc2')}</li>
              <li>{t('about.archProc3')}</li>
              <li>{t('about.archProc4')}</li>
            </ul>
          </div>
          <div className="hidden md:flex items-center justify-center text-zinc-600 text-2xl font-thin">→</div>
          {/* Deliver */}
          <div className="border border-zinc-800 rounded-xl p-5 bg-zinc-900/40">
            <p className="text-xs font-mono uppercase tracking-widest text-orange-400 mb-3">{t('about.archStep3')}</p>
            <ul className="flex flex-col gap-2 text-zinc-300 text-sm">
              <li>{t('about.archDeliver1')}</li>
              <li>{t('about.archDeliver2')}</li>
              <li>{t('about.archDeliver3')}</li>
            </ul>
          </div>
        </div>
      </Section>

      <Divider />

      {/* Data Sources - left accent */}
      <Section id="data-sources">
        <Label>{t('about.sourcesLabel')}</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-8">{t('about.sourcesTitle')}</h2>
        <div className="space-y-5">
          {sources.map(({ name, url, desc }) => (
            <div key={name} className="border-l-2 border-orange-400/30 pl-5">
              <div className="flex items-start justify-between gap-4 mb-1">
                <h3 className="text-sm font-medium text-zinc-200">{name}</h3>
                <a
                  href={url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[11px] font-mono text-zinc-600 hover:text-zinc-400 shrink-0 transition-colors"
                >
                  {url.replace('https://', '')} ↗
                </a>
              </div>
              <p className="text-zinc-400 text-sm leading-relaxed">{desc}</p>
            </div>
          ))}
        </div>
      </Section>

      <Divider />

      {/* Contact */}
      <Section id="contact">
        <Label>{t('about.contactLabel')}</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-6">{t('about.contactTitle')}</h2>
        <p className="text-zinc-400 text-sm leading-relaxed mb-6">
          {t('about.contactDesc')}
        </p>
        <a
          href="mailto:hello@astraeusio.com"
          className="inline-block font-mono text-zinc-200 hover:text-white border-b border-zinc-700 hover:border-zinc-400 pb-0.5 transition-colors text-sm"
        >
          hello@astraeusio.com
        </a>
      </Section>

      <Footer />
    </div>
  )
}
