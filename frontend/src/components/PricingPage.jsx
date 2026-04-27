import { useTranslation } from 'react-i18next'
import Navbar from './Navbar'

export default function PricingPage({ onSignIn, onSignUp }) {
  const { t } = useTranslation()
  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <Navbar onSignIn={onSignIn} />
      <div className="flex flex-col items-center justify-center min-h-screen gap-4 px-6 text-center">
        <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase">{t('landing.navPricing')}</p>
        <h1 className="text-4xl font-thin tracking-tight text-zinc-100">{t('plan.comingSoon')}</h1>
        <p className="text-zinc-500 text-sm max-w-sm">Pricing plans are being finalized. Check back soon.</p>
        <button onClick={onSignUp} className="mt-4 px-5 py-2.5 rounded-lg bg-zinc-100 text-zinc-950 text-sm font-medium hover:bg-white transition-colors">
          {t('landing.ctaPrimary')}
        </button>
      </div>
    </div>
  )
}
