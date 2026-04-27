import { useState } from 'react'
import { Link } from 'react-router-dom'
import Navbar from './Navbar'

// ── Data ─────────────────────────────────────────────────────────────────────

const PLANS = [
  {
    key:       'starter',
    name:      'Starter',
    sub:       'For individuals getting started',
    monthly:   0,
    annual:    0,
    freeLabel: 'Free',
    badge:     null,
    highlight: false,
    cta:       'Start for free',
    features: [
      '1,000 API calls / month',
      '7-day data history',
      'Dashboard access',
    ],
  },
  {
    key:       'pro',
    name:      'Pro',
    sub:       'For researchers and enthusiasts',
    monthly:   29,
    annual:    23,
    freeLabel: null,
    badge:     'Most popular',
    highlight: true,
    cta:       'Get started',
    features: [
      '10,000 API calls / month',
      '30-day data history',
      'CSV export',
      'Email alerts',
    ],
  },
  {
    key:       'business',
    name:      'Business',
    sub:       'For teams and operators',
    monthly:   99,
    annual:    79,
    freeLabel: null,
    badge:     null,
    highlight: false,
    cta:       'Get started',
    features: [
      '100,000 API calls / month',
      '90-day data history',
      'Webhooks',
      'Priority support',
    ],
  },
  {
    key:       'enterprise',
    name:      'Enterprise',
    sub:       'For critical infrastructure',
    monthly:   null,
    annual:    null,
    freeLabel: 'Custom',
    badge:     null,
    highlight: false,
    cta:       'Contact us',
    features: [
      'Unlimited API calls',
      'Unlimited data history',
      'SLA guarantee',
      'Dedicated support',
    ],
  },
]

const ROWS = [
  { label: 'API calls / month',  k: 'api',       type: 'text' },
  { label: 'Data history',       k: 'history',   type: 'text' },
  { label: 'Dashboard access',   k: 'dash',      type: 'bool' },
  { label: 'CSV export',         k: 'csv',       type: 'bool' },
  { label: 'Email alerts',       k: 'email',     type: 'bool' },
  { label: 'Webhooks',           k: 'webhooks',  type: 'bool' },
  { label: 'Priority support',   k: 'priority',  type: 'bool' },
  { label: 'SLA guarantee',      k: 'sla',       type: 'bool' },
  { label: 'Dedicated support',  k: 'dedicated', type: 'bool' },
]

const TABLE = {
  starter:    { api: '1,000',     history: '7 days',     dash: true,  csv: false, email: false, webhooks: false, priority: false, sla: false, dedicated: false },
  pro:        { api: '10,000',    history: '30 days',    dash: true,  csv: true,  email: true,  webhooks: false, priority: false, sla: false, dedicated: false },
  business:   { api: '100,000',   history: '90 days',    dash: true,  csv: true,  email: true,  webhooks: true,  priority: true,  sla: false, dedicated: false },
  enterprise: { api: 'Unlimited', history: 'Unlimited',  dash: true,  csv: true,  email: true,  webhooks: true,  priority: true,  sla: true,  dedicated: true  },
}

// ── Icons ────────────────────────────────────────────────────────────────────

function CheckIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="20 6 9 17 4 12" />
    </svg>
  )
}

// ── Toggle ───────────────────────────────────────────────────────────────────

function BillingToggle({ annual, onChange }) {
  return (
    <div className="inline-flex items-center gap-3">
      <span className={`text-sm transition-colors ${!annual ? 'text-zinc-100' : 'text-zinc-500'}`}>Monthly</span>
      <button
        onClick={onChange}
        aria-label="Toggle annual billing"
        className={`relative w-11 h-6 rounded-full transition-colors ${annual ? 'bg-orange-500' : 'bg-zinc-700'}`}
      >
        <span className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform ${annual ? 'translate-x-5' : 'translate-x-0'}`} />
      </button>
      <span className={`text-sm transition-colors ${annual ? 'text-zinc-100' : 'text-zinc-500'}`}>
        Annual
        <span className="ml-2 text-xs font-mono text-green-400">−20%</span>
      </span>
    </div>
  )
}

// ── Plan card ─────────────────────────────────────────────────────────────────

function PlanCard({ plan, annual, onCta }) {
  const price    = annual ? plan.annual : plan.monthly
  const showSave = annual && plan.monthly > 0

  return (
    <div className={`relative rounded-2xl border flex flex-col ${
      plan.highlight ? 'border-orange-500/40 bg-zinc-900' : 'border-zinc-800 bg-zinc-900'
    }`}>

      {plan.badge && (
        <div className="absolute -top-3.5 left-0 right-0 flex justify-center">
          <span className="text-xs font-mono px-3 py-1 rounded-full bg-orange-500 text-zinc-950 font-medium">
            {plan.badge}
          </span>
        </div>
      )}

      <div className="p-7 flex flex-col gap-6 flex-1">

        {/* Header */}
        <div>
          <h2 className="text-base font-semibold text-zinc-100">{plan.name}</h2>
          <p className="text-zinc-500 text-xs mt-1 leading-relaxed">{plan.sub}</p>
        </div>

        {/* Price */}
        <div>
          {plan.freeLabel ? (
            <span className="text-4xl font-thin text-zinc-100">{plan.freeLabel}</span>
          ) : (
            <div className="flex items-end gap-1.5">
              <span className="text-4xl font-thin text-zinc-100">${price}</span>
              <span className="text-zinc-500 text-sm mb-1.5">/mo</span>
            </div>
          )}
          {showSave && (
            <p className="text-zinc-600 text-xs mt-1">
              ${price * 12}/yr · <span className="line-through">${plan.monthly * 12}</span>
            </p>
          )}
        </div>

        {/* Features */}
        <ul className="flex flex-col gap-2.5 flex-1">
          {plan.features.map(f => (
            <li key={f} className="flex items-start gap-2 text-sm text-zinc-400">
              <span className="text-green-400 shrink-0 mt-0.5"><CheckIcon /></span>
              {f}
            </li>
          ))}
        </ul>

        {/* CTA */}
        <button
          onClick={onCta}
          className={`w-full py-2.5 rounded-lg text-sm font-medium transition-colors ${
            plan.highlight
              ? 'bg-orange-500 hover:bg-orange-400 text-white'
              : 'bg-zinc-800 hover:bg-zinc-700 text-zinc-100'
          }`}
        >
          {plan.cta}
        </button>

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
          Start for free. Scale as your needs grow. No hidden fees.
        </p>
        <BillingToggle annual={annual} onChange={() => setAnnual(a => !a)} />
      </section>

      {/* ── Plan cards ─────────────────────────────────────────────────────── */}
      <section className="px-6 pb-20 max-w-6xl mx-auto">
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-5 pt-5">
          {PLANS.map(plan => (
            <PlanCard key={plan.key} plan={plan} annual={annual} onCta={onSignUp} />
          ))}
        </div>
      </section>

      {/* ── Comparison table ───────────────────────────────────────────────── */}
      <section className="px-6 pb-28 max-w-6xl mx-auto">
        <h2 className="text-lg font-thin text-zinc-400 tracking-widest uppercase text-center mb-8">
          Full comparison
        </h2>

        <div className="overflow-x-auto rounded-xl border border-zinc-800">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-zinc-800">
                <th className="text-left px-6 py-4 text-zinc-500 font-normal">Feature</th>
                {PLANS.map(p => (
                  <th key={p.key} className={`px-5 py-4 font-medium text-center ${p.highlight ? 'text-orange-400' : 'text-zinc-300'}`}>
                    {p.name}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {ROWS.map((row, i) => (
                <tr key={row.k} className={`border-b border-zinc-800/50 last:border-0 ${i % 2 === 0 ? '' : 'bg-zinc-900/40'}`}>
                  <td className="px-6 py-3.5 text-zinc-400">{row.label}</td>
                  {PLANS.map(p => {
                    const val = TABLE[p.key][row.k]
                    return (
                      <td key={p.key} className="px-5 py-3.5 text-center">
                        {row.type === 'bool' ? (
                          val
                            ? <span className="text-green-400 inline-flex justify-center"><CheckIcon /></span>
                            : <span className="text-zinc-700">—</span>
                        ) : (
                          <span className={p.highlight ? 'text-zinc-100 font-medium' : 'text-zinc-400'}>
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
