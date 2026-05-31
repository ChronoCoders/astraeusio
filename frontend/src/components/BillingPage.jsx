import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useApi, authedFetch } from '../lib/useApi'
import { PLANS, PLAN_FEATURES, PLAN_COLOR, normalizePlan, planRank } from '../lib/plans'

function fmtDate(unixSec) {
  if (unixSec == null) return '-'
  const d = new Date(unixSec * 1000)
  if (Number.isNaN(d.getTime())) return '-'
  return d.toLocaleString('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false })
}

function CheckIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="20 6 9 17 4 12" />
    </svg>
  )
}

function Section({ title, children }) {
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">{title}</span>
      {children}
    </div>
  )
}

// ── Usage meter ───────────────────────────────────────────────────────────────

function UsageMeter({ usage, loading, error }) {
  const { t } = useTranslation()
  if (loading) return <p className="text-zinc-600 text-xs font-mono">{t('common.loading')}</p>
  if (error || !usage) return <p className="text-zinc-600 text-xs font-mono">{t('common.unavailable')}</p>

  const { count = 0, limit, period_end } = usage
  const unlimited = limit == null
  const pct = unlimited ? 0 : Math.min(100, Math.round((count / limit) * 100))
  const near = pct >= 90
  const barCls = near ? 'bg-red-500' : pct >= 70 ? 'bg-amber-500' : 'bg-zinc-400'

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-zinc-200 text-sm font-mono tabular-nums">
          {count.toLocaleString()}{' '}
          <span className="text-zinc-600">
            {unlimited ? `· ${t('billing.unlimited')}` : `/ ${limit.toLocaleString()}`}
          </span>
        </span>
        {!unlimited && <span className={`text-xs font-mono ${near ? 'text-red-400' : 'text-zinc-500'}`}>{pct}%</span>}
      </div>
      {!unlimited && (
        <div className="h-1.5 rounded-full bg-zinc-800 overflow-hidden">
          <div className={`h-full rounded-full transition-all ${barCls}`} style={{ width: `${pct}%` }} />
        </div>
      )}
      <span className="text-zinc-600 text-xs font-mono">{t('billing.resets')} {fmtDate(period_end)}</span>
    </div>
  )
}

// ── Plan card ─────────────────────────────────────────────────────────────────

function PlanCard({ plan, annual, currentRank, isCurrent, onDowngrade }) {
  const { t } = useTranslation()
  const price = annual ? plan.annual : plan.monthly
  const isFree = plan.monthly === 0
  const isEnterprise = plan.key === 'enterprise'
  const showSave = annual && plan.monthly > 0
  const features = PLAN_FEATURES[plan.key].map(fk => t(`pricing.features.${fk}`))
  const cls = PLAN_COLOR[plan.key] ?? PLAN_COLOR.free

  let action
  if (isCurrent) {
    action = (
      <button disabled
        className="w-full py-2.5 rounded-lg text-sm font-medium bg-zinc-800 text-zinc-500 cursor-default">
        {t('billing.current')}
      </button>
    )
  } else if (isFree) {
    action = (
      <button onClick={onDowngrade}
        className="w-full py-2.5 rounded-lg text-sm font-medium border border-zinc-600 text-zinc-300 hover:text-zinc-100 hover:border-zinc-400 transition-colors">
        {t('billing.downgradeFree')}
      </button>
    )
  } else {
    const label = isEnterprise || plan.rank < currentRank ? t('billing.contactSales') : t('billing.upgrade')
    action = (
      <a href={`mailto:altug@bytus.io?subject=Astraeusio ${plan.key.charAt(0).toUpperCase() + plan.key.slice(1)} Inquiry`}
        className={`block text-center w-full py-2.5 rounded-lg text-sm font-medium transition-colors ${
          plan.highlight
            ? 'bg-orange-500 hover:bg-orange-400 text-white'
            : 'border border-zinc-600 text-zinc-300 hover:text-zinc-100 hover:border-zinc-400'
        }`}>
        {label}
      </a>
    )
  }

  return (
    <div className={`relative flex flex-col rounded-2xl border p-5 gap-4 ${
      isCurrent ? 'border-zinc-500 bg-zinc-900' : plan.highlight ? 'border-orange-500/40 bg-zinc-900' : 'border-zinc-800 bg-zinc-900'
    }`}>
      {isCurrent && (
        <span className="absolute -top-2.5 left-4 text-[10px] font-mono uppercase tracking-wide px-2 py-0.5 rounded-full bg-zinc-100 text-zinc-950">
          {t('billing.currentBadge')}
        </span>
      )}
      <div className="flex items-center gap-2">
        <span className={`text-xs font-mono border rounded px-1.5 py-0.5 ${cls}`}>{t(`plan.${plan.key}`)}</span>
      </div>
      <p className="text-xs text-zinc-500 leading-relaxed min-h-[2rem]">{t(`pricing.plans.${plan.key}.sub`)}</p>
      <div>
        {isEnterprise ? (
          <span className="text-2xl font-thin text-zinc-100">{t('pricing.plans.enterprise.price')}</span>
        ) : (
          <div className="flex items-end gap-1">
            <span className="text-2xl font-thin text-zinc-100 tabular-nums">${price}</span>
            {!isFree && <span className="text-zinc-500 text-xs mb-1">{t('pricing.perMo')}</span>}
          </div>
        )}
        {showSave && <p className="text-zinc-600 text-xs mt-0.5">${price * 12}/yr</p>}
      </div>
      <ul className="flex flex-col gap-1.5 flex-1">
        {features.map(f => (
          <li key={f} className="flex items-start gap-2 text-xs text-zinc-400">
            <span className="text-green-400 shrink-0 mt-0.5"><CheckIcon /></span>{f}
          </li>
        ))}
      </ul>
      <div className="mt-auto">{action}</div>
    </div>
  )
}

// ── Page ────────────────────────────────────────────────────────────────────

export default function BillingPage({ user, onUserChange }) {
  const { t } = useTranslation()
  const usage = useApi('/api/usage', 60_000)
  const [annual, setAnnual] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const [saving, setSaving] = useState(false)
  const [err, setErr] = useState(null)
  const [done, setDone] = useState(false)

  const plan = user?.plan ?? 'starter'
  const effective = normalizePlan(plan)
  const currentRank = planRank(plan)
  const planCls = PLAN_COLOR[effective] ?? PLAN_COLOR.free

  async function downgradeToFree() {
    setSaving(true)
    setErr(null)
    try {
      const r = await authedFetch('/api/user/plan', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ plan: 'free' }),
      })
      if (r.status === 204) {
        onUserChange?.({ ...user, plan: 'free' })
        setConfirming(false)
        setDone(true)
      } else {
        const d = await r.json().catch(() => ({}))
        setErr(d.error ?? t('auth.unknownError'))
      }
    } catch {
      setErr(t('auth.networkError'))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="flex flex-col gap-6 max-w-6xl">
      <div>
        <span className="text-zinc-100 text-lg font-thin tracking-wide">{t('billing.title')}</span>
        <p className="text-zinc-500 text-sm mt-1">{t('billing.subtitle')}</p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        <Section title={t('billing.currentPlan')}>
          <div className="flex items-center gap-3">
            <span className={`text-xs font-mono border rounded px-2 py-0.5 ${planCls}`}>{t(`plan.${effective}`)}</span>
            <span className="text-zinc-500 text-xs font-mono">{user?.email}</span>
          </div>
          {done && <p className="text-green-400 text-xs font-mono">{t('billing.downgraded')}</p>}
        </Section>

        <Section title={t('billing.usageTitle')}>
          <UsageMeter usage={usage.data} loading={usage.loading} error={usage.error} />
        </Section>
      </div>

      {/* ── Tier picker ─────────────────────────────────────────────── */}
      <div className="flex items-center justify-between flex-wrap gap-3">
        <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">{t('billing.choosePlan')}</span>
        <div className="inline-flex items-center gap-2.5 text-xs">
          <span className={!annual ? 'text-zinc-200' : 'text-zinc-500'}>{t('pricing.monthly')}</span>
          <button onClick={() => setAnnual(a => !a)} aria-label="Toggle annual billing"
            className={`relative w-10 h-5 rounded-full transition-colors ${annual ? 'bg-orange-500' : 'bg-zinc-700'}`}>
            <span className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform ${annual ? 'translate-x-5' : ''}`} />
          </button>
          <span className={annual ? 'text-zinc-200' : 'text-zinc-500'}>
            {t('pricing.annual')} <span className="text-green-400 ml-1">{t('pricing.save')}</span>
          </span>
        </div>
      </div>

      {err && <p className="text-red-400 text-xs font-mono">{err}</p>}

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5 gap-3">
        {PLANS.map(p => (
          <PlanCard
            key={p.key}
            plan={p}
            annual={annual}
            currentRank={currentRank}
            isCurrent={p.key === effective}
            onDowngrade={() => { setErr(null); setDone(false); setConfirming(true) }}
          />
        ))}
      </div>

      <p className="text-zinc-600 text-xs">{t('billing.footnote')}</p>

      {/* ── Downgrade confirmation ──────────────────────────────────── */}
      {confirming && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm" onClick={() => setConfirming(false)}>
          <div className="bg-zinc-900 border border-zinc-700 rounded-xl p-6 w-full max-w-sm mx-4 flex flex-col gap-4" onClick={e => e.stopPropagation()}>
            <span className="text-zinc-100 text-sm font-mono">{t('billing.confirmTitle')}</span>
            <p className="text-zinc-400 text-xs leading-relaxed">{t('billing.confirmBody')}</p>
            {err && <p className="text-red-400 text-xs font-mono">{err}</p>}
            <div className="flex gap-2">
              <button onClick={downgradeToFree} disabled={saving}
                className="flex-1 py-2 text-xs font-mono rounded bg-zinc-100 text-zinc-950 hover:bg-white disabled:bg-zinc-700 disabled:text-zinc-500 transition-colors">
                {saving ? t('common.loading') : t('billing.confirmDowngrade')}
              </button>
              <button onClick={() => setConfirming(false)}
                className="px-4 py-2 text-xs font-mono rounded border border-zinc-700 text-zinc-500 hover:text-zinc-300 transition-colors">
                {t('billing.cancel')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
