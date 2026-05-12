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

export default function AboutPage({ onSignIn }) {
  const { t } = useTranslation()

  const sources = [
    {
      name: 'NOAA Space Weather Prediction Center',
      url: 'https://www.swpc.noaa.gov',
      desc: t('about.src1Desc'),
    },
    {
      name: 'NASA APIs',
      url: 'https://api.nasa.gov',
      desc: t('about.src2Desc'),
    },
    {
      name: 'Celestrak',
      url: 'https://celestrak.org',
      desc: t('about.src3Desc'),
    },
    {
      name: 'Kyoto World Data Center for Geomagnetism',
      url: 'https://wdc.kugi.kyoto-u.ac.jp',
      desc: t('about.src4Desc'),
    },
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
      </Section>

      <Divider />

      {/* How it's built */}
      <Section id="built">
        <Label>{t('about.builtLabel')}</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-6">{t('about.builtTitle')}</h2>
        <div className="space-y-4 text-zinc-400 text-sm leading-relaxed">
          <p>{t('about.builtP1')}</p>
          <p>{t('about.builtP2')}</p>
          <p>{t('about.builtP3')}</p>
        </div>
      </Section>

      <Divider />

      {/* Data Sources */}
      <Section id="data-sources">
        <Label>{t('about.sourcesLabel')}</Label>
        <h2 className="text-2xl font-light text-zinc-100 mb-8">{t('about.sourcesTitle')}</h2>
        <div className="space-y-6">
          {sources.map(({ name, url, desc }) => (
            <div key={name} className="border border-zinc-800 rounded-lg p-5 bg-zinc-900/30">
              <div className="flex items-start justify-between gap-4 mb-3">
                <h3 className="text-sm font-medium text-zinc-200">{name}</h3>
                <a
                  href={url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[11px] font-mono text-zinc-500 hover:text-zinc-300 shrink-0 transition-colors"
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
          href="mailto:contact@chronocoder.dev"
          className="inline-block font-mono text-zinc-200 hover:text-white border-b border-zinc-700 hover:border-zinc-400 pb-0.5 transition-colors text-sm"
        >
          contact@chronocoder.dev
        </a>
      </Section>

      <Footer />
    </div>
  )
}
