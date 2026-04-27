import { useState } from 'react'
import { Link } from 'react-router-dom'
import Navbar from './Navbar'

// ── Plans ─────────────────────────────────────────────────────────────────────

const PLANS = [
  {
    key:       'free',
    name:      'Free',
    sub:       'Explore space weather data',
    monthly:   0,
    annual:    0,
    freeLabel: '$0',
    badge:     null,
    highlight: false,
    cta:       'Get started',
    trust:     ['No credit card required', 'No lock-in', 'Cancel anytime'],
    features: [
      '100 requests / day',
      '60s+ data delay',
      'Kp & solar wind data',
    ],
  },
  {
    key:       'developer',
    name:      'Developer',
    sub:       'Build and prototype',
    monthly:   29,
    annual:    23,
    freeLabel: null,
    badge:     null,
    highlight: false,
    cta:       'Get API Access',
    trust:     [],
    features: [
      '10,000 requests / month',
      'Real-time data',
      'ML forecast',
      'Basic anomaly detection',
      'Limited email alerts',
    ],
  },
  {
    key:       'pro',
    name:      'Pro',
    sub:       'Production-grade monitoring',
    monthly:   99,
    annual:    79,
    freeLabel: null,
    badge:     'Most Popular',
    highlight: true,
    cta:       'Start building',
    trust:     [],
    features: [
      '100,000 requests / month',
      'Real-time data',
      'ML forecast + confidence intervals',
      'Full anomaly detection',
      'Webhook alerts',
      'Priority support',
    ],
  },
  {
    key:       'business',
    name:      'Business',
    sub:       'Scale with confidence',
    monthly:   299,
    annual:    239,
    freeLabel: null,
    badge:     null,
    highlight: false,
    cta:       'Scale your system',
    trust:     [],
    features: [
      '1,000,000 requests / month',
      'Real-time data',
      'Advanced alerting',
      'Custom thresholds',
      'Multi-channel alerts',
      'SLA-backed uptime',
    ],
  },
  {
    key:       'enterprise',
    name:      'Enterprise',
    sub:       'Mission-critical operations',
    monthly:   null,
    annual:    null,
    freeLabel: 'Custom',
    badge:     null,
    highlight: false,
    cta:       'Contact sales',
    trust:     [],
    features: [
      'Unlimited requests',
      'Dedicated infrastructure',
      'Custom anomaly models',
      'SLA + onboarding',
      'Dedicated support',
    ],
  },
]

// ── Comparison table ──────────────────────────────────────────────────────────

const ROWS = [
  { label: 'API requests',           k: 'api',       type: 'text' },
  { label: 'Data delay',             k: 'delay',     type: 'text' },
  { label: 'Kp & solar data',        k: 'kp',        type: 'bool' },
  { label: 'Real-time data',         k: 'realtime',  type: 'bool' },
  { label: 'ML forecast',            k: 'ml',        type: 'bool' },
  { label: 'Confidence intervals',   k: 'ci',        type: 'bool' },
  { label: 'Basic anomaly detection',k: 'anomalyBasic', type: 'bool' },
  { label: 'Full anomaly detection', k: 'anomalyFull',  type: 'bool' },
  { label: 'Custom thresholds',      k: 'thresholds',   type: 'bool' },
  { label: 'Email alerts',           k: 'email',     type: 'text' },
  { label: 'Webhook alerts',         k: 'webhooks',  type: 'bool' },
  { label: 'Multi-channel alerts',   k: 'multichan', type: 'bool' },
  { label: 'Custom anomaly models',  k: 'customModels', type: 'bool' },
  { label: 'SLA uptime',             k: 'sla',       type: 'bool' },
  { label: 'Dedicated infrastructure',k: 'dedicated',type: 'bool' },
  { label: 'Support',                k: 'support',   type: 'text' },
]

const TABLE = {
  free:       { api: '100 / day',   delay: '60s+',       kp: true,  realtime: false, ml: false, ci: false, anomalyBasic: false, anomalyFull: false, thresholds: false, email: '—',       webhooks: false, multichan: false, customModels: false, sla: false, dedicated: false, support: 'Community' },
  developer:  { api: '10K / mo',    delay: 'Real-time',  kp: true,  realtime: true,  ml: true,  ci: false, anomalyBasic: true,  anomalyFull: false, thresholds: false, email: 'Limited', webhooks: false, multichan: false, customModels: false, sla: false, dedicated: false, support: 'Email'     },
  pro:        { api: '100K / mo',   delay: 'Real-time',  kp: true,  realtime: true,  ml: true,  ci: true,  anomalyBasic: true,  anomalyFull: true,  thresholds: false, email: '✓',       webhooks: true,  multichan: false, customModels: false, sla: false, dedicated: false, support: 'Priority'  },
  business:   { api: '1M / mo',     delay: 'Real-time',  kp: true,  realtime: true,  ml: true,  ci: true,  anomalyBasic: true,  anomalyFull: true,  thresholds: true,  email: '✓',       webhooks: true,  multichan: true,  customModels: false, sla: true,  dedicated: false, support: 'Priority'  },
  enterprise: { api: 'Unlimited',   delay: 'Real-time',  kp: true,  realtime: true,  ml: true,  ci: true,  anomalyBasic: true,  anomalyFull: true,  thresholds: true,  email: '✓',       webhooks: true,  multichan: true,  customModels: true,  sla: true,  dedicated: true,  support: 'Dedicated' },
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
  return (
    <div className="inline-flex items-center gap-3">
      <span className={`text-sm transition-colors ${!annual ? 'text-zinc-100' : 'text-zinc-500'}`}>
        Monthly
      </span>
      <button
        onClick={onChange}
        aria-label="Toggle annual billing"
        className={`relative w-11 h-6 rounded-full transition-colors ${annual ? 'bg-orange-500' : 'bg-zinc-700'}`}
      >
        <span className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform ${annual ? 'translate-x-5' : ''}`} />
      </button>
      <span className={`text-sm transition-colors ${annual ? 'text-zinc-100' : 'text-zinc-500'}`}>
        Annual
        <span className="ml-2 text-xs font-mono text-green-400">−20%</span>
      </span>
    </div>
  )
}

function PlanCard({ plan, annual, onCta }) {
  const price    = annual ? plan.annual : plan.monthly
  const showSave = annual && plan.monthly > 0

  return (
    <div className={`relative flex flex-col rounded-2xl border ${
      plan.highlight
        ? 'border-orange-500/50 bg-zinc-900 shadow-[0_0_40px_-8px_rgba(249,115,22,0.25)]'
        : 'border-zinc-800 bg-zinc-900'
    }`}>

      {plan.badge && (
        <div className="absolute -top-3.5 inset-x-0 flex justify-center">
          <span className="text-xs font-mono font-medium px-3 py-1 rounded-full bg-orange-500 text-zinc-950">
            {plan.badge}
          </span>
        </div>
      )}

      <div className="p-6 flex flex-col gap-5 flex-1">

        {/* Header */}
        <div>
          <p className="text-base font-semibold text-zinc-100">{plan.name}</p>
          <p className="text-xs text-zinc-500 mt-0.5 leading-relaxed">{plan.sub}</p>
        </div>

        {/* Price */}
        <div>
          {plan.freeLabel ? (
            <span className="text-3xl font-thin text-zinc-100">{plan.freeLabel}</span>
          ) : (
            <div className="flex items-end gap-1">
              <span className="text-3xl font-thin text-zinc-100">${price}</span>
              <span className="text-zinc-500 text-xs mb-1.5">/mo</span>
            </div>
          )}
          {showSave && (
            <p className="text-zinc-600 text-xs mt-1">
              ${price * 12}/yr · <span className="line-through">${plan.monthly * 12}</span>
            </p>
          )}
          {plan.freeLabel === '$0' && !showSave && (
            <p className="text-zinc-700 text-xs mt-1 invisible">—</p>
          )}
        </div>

        {/* Feature list */}
        <ul className="flex flex-col gap-2 flex-1">
          {plan.features.map(f => (
            <li key={f} className="flex items-start gap-2 text-xs text-zinc-400">
              <span className="text-green-400 shrink-0 mt-0.5"><CheckIcon /></span>
              {f}
            </li>
          ))}
        </ul>

        {/* CTA */}
        <div className="flex flex-col gap-2 mt-auto">
          <button
            onClick={onCta}
            className={`w-full py-2.5 rounded-lg text-sm font-medium transition-colors ${
              plan.highlight
                ? 'bg-orange-500 hover:bg-orange-400 text-white'
                : plan.key === 'enterprise'
                ? 'border border-zinc-600 hover:border-zinc-400 text-zinc-300 hover:text-zinc-100 bg-transparent'
                : 'bg-zinc-800 hover:bg-zinc-700 text-zinc-100'
            }`}
          >
            {plan.cta}
          </button>

          {plan.trust.length > 0 && (
            <div className="flex flex-col gap-1 pt-1">
              {plan.trust.map(s => (
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
  const [annual, setAnnual] = useState(false)

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <Navbar onSignIn={onSignIn} />

      {/* ── Hero ───────────────────────────────────────────────────────────── */}
      <section className="pt-36 pb-14 px-6 text-center">
        <p className="text-xs font-mono tracking-[0.2em] text-orange-400 uppercase mb-4">Pricing</p>
        <h1 className="text-4xl md:text-5xl font-thin tracking-tight text-zinc-100 max-w-2xl mx-auto mb-5">
          Simple, transparent pricing.
        </h1>
        <p className="text-zinc-400 text-base max-w-md mx-auto mb-10 leading-relaxed">
          Start free. Scale when you&apos;re ready. No hidden fees.
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
          Full comparison
        </p>

        <div className="overflow-x-auto rounded-xl border border-zinc-800">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-zinc-800">
                <th className="text-left px-5 py-4 text-zinc-500 font-normal w-48">Feature</th>
                {PLANS.map(p => (
                  <th key={p.key} className={`px-4 py-4 text-center font-medium text-xs uppercase tracking-wide ${
                    p.highlight ? 'text-orange-400' : 'text-zinc-400'
                  }`}>
                    {p.name}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {ROWS.map((row, i) => (
                <tr key={row.k} className={`border-b border-zinc-800/50 last:border-0 ${
                  i % 2 !== 0 ? 'bg-zinc-900/30' : ''
                }`}>
                  <td className="px-5 py-3 text-zinc-400 text-xs">{row.label}</td>
                  {PLANS.map(p => {
                    const val = TABLE[p.key][row.k]
                    return (
                      <td key={p.key} className="px-4 py-3 text-center">
                        {row.type === 'bool' ? (
                          val
                            ? <span className="text-green-400 inline-flex justify-center"><CheckIcon /></span>
                            : <span className="text-zinc-700 text-xs">—</span>
                        ) : (
                          <span className={`text-xs ${
                            p.highlight ? 'text-zinc-100 font-medium' : 'text-zinc-400'
                          } ${val === '—' ? 'text-zinc-700' : ''}`}>
                            {val}
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
          Questions?{' '}
          <button onClick={onSignUp} className="text-zinc-300 hover:text-white underline underline-offset-4 transition-colors">
            Talk to us
          </button>
          {' '}— we&apos;ll help you pick the right plan.
        </p>
      </section>

      {/* ── Footer ─────────────────────────────────────────────────────────── */}
      <footer className="border-t border-zinc-800 px-6 py-10 text-center">
        <Link to="/" className="text-zinc-500 hover:text-zinc-300 text-sm transition-colors">
          ← Back to home
        </Link>
        <p className="text-zinc-700 text-xs mt-4">
          Astraeusio · Built on open scientific data · Powered by NOAA, NASA, and open-access space weather APIs · BSL 1.1
        </p>
      </footer>
    </div>
  )
}
