// Shared subscription-tier data - single source of truth for PricingPage (public)
// and BillingPage (dashboard). Keep prices/features here so the two stay in sync.
//
// `monthly`/`annual` are USD per month; enterprise is quote-only (null).
// `rank` orders the tiers (free=0 … enterprise=4); `highlight` marks the featured plan.
// There is no payment processor: paid tiers are acquired via sales inquiry; only a
// self-serve downgrade to Free hits the API.

export const PLANS = [
  { key: 'free',       monthly: 0,    annual: 0,    rank: 0, highlight: false },
  { key: 'developer',  monthly: 29,   annual: 23,   rank: 1, highlight: false },
  { key: 'pro',        monthly: 99,   annual: 79,   rank: 2, highlight: true  },
  { key: 'business',   monthly: 299,  annual: 239,  rank: 3, highlight: false },
  { key: 'enterprise', monthly: null, annual: null, rank: 4, highlight: false },
]

// i18n keys under `pricing.features.*`
export const PLAN_FEATURES = {
  free:       ['req100day', 'delay60', 'kpSolar'],
  developer:  ['req10k', 'realtime', 'ml', 'anomalyBasic', 'emailLimited'],
  pro:        ['req100k', 'realtime', 'mlCI', 'anomalyFull', 'webhooks', 'prioritySupport'],
  business:   ['req1m', 'realtime', 'advAlerts', 'thresholds', 'multiChannel', 'sla'],
  enterprise: ['unlimited', 'dedicated', 'customModels', 'slaOnboarding', 'dedicatedSupport'],
}

// Badge colors keyed by tier.
export const PLAN_COLOR = {
  free:       'border-zinc-700 text-zinc-400',
  developer:  'border-blue-700 text-blue-400',
  pro:        'border-purple-700 text-purple-400',
  business:   'border-amber-700 text-amber-400',
  enterprise: 'border-orange-600 text-orange-400',
}

// The default account plan is "starter"; the public tiers call it "free".
export const normalizePlan = (plan) => (plan === 'starter' ? 'free' : (plan ?? 'free'))

export const planRank = (plan) =>
  PLANS.find(p => p.key === normalizePlan(plan))?.rank ?? 0
