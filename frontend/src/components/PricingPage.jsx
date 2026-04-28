import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import Navbar from './Navbar'

// ── Static data (keys only, no human-readable strings) ────────────────────────

const PLANS = [
  { key: 'free',       monthly: 0,    annual: 0,    highlight: false },
  { key: 'developer',  monthly: 29,   annual: 23,   highlight: false },
  { key: 'pro',        monthly: 99,   annual: 79,   highlight: true  },
  { key: 'business',   monthly: 299,  annual: 239,  highlight: false },
  { key: 'enterprise', monthly: null, annual: null, highlight: false },
]

const PLAN_FEATURES = {
  free:       ['req100day', 'delay60', 'kpSolar'],
  developer:  ['req10k', 'realtime', 'ml', 'anomalyBasic', 'emailLimited'],
  pro:        ['req100k', 'realtime', 'mlCI', 'anomalyFull', 'webhooks', 'prioritySupport'],
  business:   ['req1m', 'realtime', 'advAlerts', 'thresholds', 'multiChannel', 'sla'],
  enterprise: ['unlimited', 'dedicated', 'customModels', 'slaOnboarding', 'dedicatedSupport'],
}

const ROWS = [
  { k: 'api',          type: 'text' },
  { k: 'delay',        type: 'text' },
  { k: 'kp',           type: 'bool' },
  { k: 'realtime',     type: 'bool' },
  { k: 'ml',           type: 'bool' },
  { k: 'ci',           type: 'bool' },
  { k: 'anomalyBasic', type: 'bool' },
  { k: 'anomalyFull',  type: 'bool' },
  { k: 'thresholds',   type: 'bool' },
  { k: 'email',        type: 'text' },
  { k: 'webhooks',     type: 'bool' },
  { k: 'multichan',    type: 'bool' },
  { k: 'customModels', type: 'bool' },
  { k: 'sla',          type: 'bool' },
  { k: 'dedicated',    type: 'bool' },
  { k: 'support',      type: 'text' },
]

// Text cells: true = ✓, null/false = —, string = t('pricing.tv.' + val)
const TABLE = {
  free:       { api: 'req100day', delay: 'delay60',  kp: true,  realtime: false, ml: false, ci: false, anomalyBasic: false, anomalyFull: false, thresholds: false, email: null,      webhooks: false, multichan: false, customModels: false, sla: false, dedicated: false, support: 'community' },
  developer:  { api: 'req10k',    delay: 'realtime', kp: true,  realtime: true,  ml: true,  ci: false, anomalyBasic: true,  anomalyFull: false, thresholds: false, email: 'limited', webhooks: false, multichan: false, customModels: false, sla: false, dedicated: false, support: 'email'     },
  pro:        { api: 'req100k',   delay: 'realtime', kp: true,  realtime: true,  ml: true,  ci: true,  anomalyBasic: true,  anomalyFull: true,  thresholds: false, email: true,      webhooks: true,  multichan: false, customModels: false, sla: false, dedicated: false, support: 'priority'  },
  business:   { api: 'req1m',     delay: 'realtime', kp: true,  realtime: true,  ml: true,  ci: true,  anomalyBasic: true,  anomalyFull: true,  thresholds: true,  email: true,      webhooks: true,  multichan: true,  customModels: false, sla: true,  dedicated: false, support: 'priority'  },
  enterprise: { api: 'unlimited', delay: 'realtime', kp: true,  realtime: true,  ml: true,  ci: true,  anomalyBasic: true,  anomalyFull: true,  thresholds: true,  email: true,      webhooks: true,  multichan: true,  customModels: true,  sla: true,  dedicated: true,  support: 'dedicated' },
}

// ── Sub-components ────────────────────────────────────────────────────────────

function CheckIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="20 6 9 17 4 12" />
    </svg>
  )
}

function BillingToggle({ annual, onChange }) {
  const { t } = useTranslation()
  return (
    <div className="inline-flex items-center gap-3">
      <span className={`text-sm transition-colors ${!annual ? 'text-zinc-100' : 'text-zinc-500'}`}>
        {t('pricing.monthly')}
      </span>
      <button
        onClick={onChange}
        aria-label="Toggle annual billing"
        className={`relative w-11 h-6 rounded-full transition-colors ${annual ? 'bg-orange-500' : 'bg-zinc-700'}`}
      >
        <span className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform ${annual ? 'translate-x-5' : ''}`} />
      </button>
      <span className={`text-sm transition-colors ${annual ? 'text-zinc-100' : 'text-zinc-500'}`}>
        {t('pricing.annual')}
        <span className="ml-2 text-xs font-mono text-green-400">{t('pricing.save')}</span>
      </span>
    </div>
  )
}

function PlanCard({ plan, annual, onCta }) {
  const { t } = useTranslation()
  const price      = annual ? plan.annual : plan.monthly
  const showSave   = annual && plan.monthly > 0
  const isFree     = plan.monthly === 0
  const isEnterprise = plan.key === 'enterprise'
  const features   = PLAN_FEATURES[plan.key].map(fk => t(`pricing.features.${fk}`))
  const trust      = isFree ? ['trust1', 'trust2', 'trust3'].map(k => t(`pricing.plans.free.${k}`)) : []

  return (
    <div className={`relative flex flex-col rounded-2xl border ${
      plan.highlight
        ? 'border-orange-500/50 bg-zinc-900 shadow-[0_0_40px_-8px_rgba(249,115,22,0.25)]'
        : 'border-zinc-800 bg-zinc-900'
    }`}>

      {plan.highlight && (
        <div className="absolute -top-3.5 inset-x-0 flex justify-center">
          <span className="text-xs font-mono font-medium px-3 py-1 rounded-full bg-orange-500 text-zinc-950">
            {t('pricing.mostPopular')}
          </span>
        </div>
      )}

      <div className="p-6 flex flex-col gap-5 flex-1">

        <div>
          <p className="text-base font-semibold text-zinc-100">{t(`pricing.plans.${plan.key}.name`)}</p>
          <p className="text-xs text-zinc-500 mt-0.5 leading-relaxed">{t(`pricing.plans.${plan.key}.sub`)}</p>
        </div>

        <div>
          {(isFree || isEnterprise) ? (
            <span className="text-3xl font-thin text-zinc-100">
              {isEnterprise ? t('pricing.plans.enterprise.price') : '$0'}
            </span>
          ) : (
            <div className="flex items-end gap-1">
              <span className="text-3xl font-thin text-zinc-100">${price}</span>
              <span className="text-zinc-500 text-xs mb-1.5">{t('pricing.perMo')}</span>
            </div>
          )}
          {showSave && (
            <p className="text-zinc-600 text-xs mt-1">
              ${price * 12}/yr · <span className="line-through">${plan.monthly * 12}</span>
            </p>
          )}
          {isFree && !showSave && (
            <p className="text-zinc-700 text-xs mt-1 invisible">—</p>
          )}
        </div>

        <ul className="flex flex-col gap-2 flex-1">
          {features.map(f => (
            <li key={f} className="flex items-start gap-2 text-xs text-zinc-400">
              <span className="text-green-400 shrink-0 mt-0.5"><CheckIcon /></span>
              {f}
            </li>
          ))}
        </ul>

        <div className="flex flex-col gap-2 mt-auto">
          <button
            onClick={onCta}
            className={`w-full py-2.5 rounded-lg text-sm font-medium transition-colors ${
              plan.highlight
                ? 'bg-orange-500 hover:bg-orange-400 text-white'
                : isEnterprise
                ? 'border border-zinc-600 hover:border-zinc-400 text-zinc-300 hover:text-zinc-100 bg-transparent'
                : 'bg-zinc-800 hover:bg-zinc-700 text-zinc-100'
            }`}
          >
            {t(`pricing.plans.${plan.key}.cta`)}
          </button>

          {trust.length > 0 && (
            <div className="flex flex-col gap-1 pt-1">
              {trust.map(s => (
                <p key={s} className="text-zinc-600 text-xs text-center">{s}</p>
              ))}
            </div>
          )}
        </div>

      </div>
    </div>
  )
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function PricingPage({ onSignIn, onSignUp }) {
  const { t } = useTranslation()
  const [annual, setAnnual] = useState(false)

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <Navbar onSignIn={onSignIn} />

      {/* ── Hero ───────────────────────────────────────────────────────────── */}
      <section className="pt-36 pb-14 px-6 text-center">
        <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase mb-4">
          {t('pricing.eyebrow')}
        </p>
        <h1 className="text-4xl md:text-5xl font-thin tracking-tight text-zinc-100 max-w-2xl mx-auto mb-5">
          {t('pricing.title')}
        </h1>
        <p className="text-zinc-400 text-base max-w-md mx-auto mb-10 leading-relaxed">
          {t('pricing.sub')}
        </p>
        <BillingToggle annual={annual} onChange={() => setAnnual(a => !a)} />
      </section>

      {/* ── Plan cards ─────────────────────────────────────────────────────── */}
      <section className="px-6 pb-24 max-w-7xl mx-auto">
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5 gap-4 pt-5">
          {PLANS.map(plan => (
            <PlanCard key={plan.key} plan={plan} annual={annual} onCta={onSignUp} />
          ))}
        </div>
      </section>

      {/* ── Comparison table ───────────────────────────────────────────────── */}
      <section className="px-6 pb-28 max-w-7xl mx-auto">
        <p className="text-xs font-mono tracking-[0.25em] text-zinc-500 uppercase text-center mb-8">
          {t('pricing.comparison')}
        </p>

        <div className="overflow-x-auto rounded-xl border border-zinc-800">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-zinc-800">
                <th className="text-left px-5 py-4 text-zinc-500 font-normal w-48">
                  {t('pricing.featureCol')}
                </th>
                {PLANS.map(p => (
                  <th key={p.key} className={`px-4 py-4 text-center font-medium text-xs uppercase tracking-wide ${
                    p.highlight ? 'text-orange-400' : 'text-zinc-400'
                  }`}>
                    {t(`pricing.plans.${p.key}.name`)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {ROWS.map((row, i) => (
                <tr key={row.k} className={`border-b border-zinc-800/50 last:border-0 ${
                  i % 2 !== 0 ? 'bg-zinc-900/30' : ''
                }`}>
                  <td className="px-5 py-3 text-zinc-400 text-xs">{t(`pricing.rows.${row.k}`)}</td>
                  {PLANS.map(p => {
                    const val = TABLE[p.key][row.k]
                    return (
                      <td key={p.key} className="px-4 py-3 text-center">
                        {row.type === 'bool' ? (
                          val
                            ? <span className="text-green-400 inline-flex justify-center"><CheckIcon /></span>
                            : <span className="text-zinc-700 text-xs">—</span>
                        ) : val === true ? (
                          <span className="text-green-400 inline-flex justify-center"><CheckIcon /></span>
                        ) : val == null || val === false ? (
                          <span className="text-zinc-700 text-xs">—</span>
                        ) : (
                          <span className={`text-xs ${
                            p.highlight ? 'text-zinc-100 font-medium' : 'text-zinc-400'
                          }`}>
                            {t(`pricing.tv.${val}`)}
                          </span>
                        )}
                      </td>
                    )
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      {/* ── FAQ nudge ──────────────────────────────────────────────────────── */}
      <section className="pb-20 px-6 text-center">
        <p className="text-zinc-500 text-sm">
          {t('pricing.faq')}{' '}
          <button onClick={onSignUp} className="text-zinc-300 hover:text-white underline underline-offset-4 transition-colors">
            {t('pricing.faqLink')}
          </button>
          {' '}{t('pricing.faqSuffix')}
        </p>
      </section>

      {/* ── Footer ─────────────────────────────────────────────────────────── */}
      <footer className="border-t border-zinc-800 px-6 py-10 text-center">
        <Link to="/" className="text-zinc-500 hover:text-zinc-300 text-sm transition-colors">
          {t('products.backHome')}
        </Link>
        <p className="text-zinc-700 text-xs mt-4">
          {t('landing.footerNote')}
        </p>
      </footer>
    </div>
  )
}
