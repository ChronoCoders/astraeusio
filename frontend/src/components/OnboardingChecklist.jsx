import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { authedFetch } from '../lib/useApi'

const DISMISS_KEY = 'astraeus.onboarding.dismissed'

export default function OnboardingChecklist({ user, setPage }) {
  const { t } = useTranslation()
  const [keys, setKeys] = useState(null)
  const [dismissed, setDismissed] = useState(
    () => localStorage.getItem(DISMISS_KEY) === '1',
  )

  useEffect(() => {
    if (dismissed) return
    let cancelled = false
    authedFetch('/api/keys')
      .then(r => (r.ok ? r.json() : []))
      .then(j => { if (!cancelled) setKeys(Array.isArray(j) ? j : []) })
      .catch(() => { if (!cancelled) setKeys([]) })
    return () => { cancelled = true }
  }, [dismissed])

  if (dismissed || !user || keys === null) return null

  const steps = [
    {
      key: 'verify',
      done: !!user.email_verified,
      title: t('onboarding.verifyTitle'),
      desc: t('onboarding.verifyDesc'),
      cta: t('onboarding.verifyCta'),
      action: () => setPage('settings'),
    },
    {
      key: 'apikey',
      done: keys.length > 0,
      title: t('onboarding.keyTitle'),
      desc: t('onboarding.keyDesc'),
      cta: t('onboarding.keyCta'),
      action: () => setPage('api'),
    },
    {
      key: 'request',
      done: keys.some(k => (k.request_count ?? 0) > 0),
      title: t('onboarding.requestTitle'),
      desc: t('onboarding.requestDesc'),
      cta: t('onboarding.requestCta'),
      action: () => window.open('/docs#quick-start', '_blank', 'noopener,noreferrer'),
    },
  ]

  const completed = steps.filter(s => s.done).length
  const total = steps.length

  if (completed === total) {
    localStorage.setItem(DISMISS_KEY, '1')
    return null
  }

  function dismiss() {
    localStorage.setItem(DISMISS_KEY, '1')
    setDismissed(true)
  }

  return (
    <div className="bg-gradient-to-br from-zinc-900 via-zinc-900 to-orange-950/30 border border-orange-500/20 rounded-lg p-5">
      <div className="flex items-start justify-between gap-4 mb-4">
        <div>
          <p className="text-xs font-mono uppercase tracking-widest text-orange-400 mb-1">
            {t('onboarding.eyebrow')}
          </p>
          <h3 className="text-zinc-100 text-base font-medium">{t('onboarding.heading')}</h3>
          <p className="text-zinc-500 text-xs mt-1">
            {t('onboarding.progress', { done: completed, total })}
          </p>
        </div>
        <button
          onClick={dismiss}
          aria-label={t('onboarding.dismiss')}
          className="text-zinc-600 hover:text-zinc-200 text-xl leading-none p-1 -m-1 shrink-0 transition-colors"
        >
          ×
        </button>
      </div>

      <div className="h-1 bg-zinc-800 rounded-full overflow-hidden mb-4">
        <div
          className="h-full bg-orange-500 transition-all duration-500"
          style={{ width: `${(completed / total) * 100}%` }}
        />
      </div>

      <ul className="divide-y divide-zinc-800/60">
        {steps.map(s => (
          <li key={s.key} className="py-3 flex items-start gap-3">
            <span
              className={`w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold shrink-0 mt-0.5 ${
                s.done
                  ? 'bg-green-500/15 text-green-400 border border-green-500/30'
                  : 'border border-zinc-700 text-zinc-700'
              }`}
            >
              {s.done ? '✓' : ''}
            </span>
            <div className="flex-1 min-w-0">
              <p className={`text-sm ${s.done ? 'text-zinc-500 line-through' : 'text-zinc-100'}`}>
                {s.title}
              </p>
              <p className="text-xs text-zinc-500 mt-0.5">{s.desc}</p>
            </div>
            {!s.done && (
              <button
                onClick={s.action}
                className="text-xs px-3 py-1.5 rounded border border-zinc-700 hover:border-zinc-400 text-zinc-300 hover:text-zinc-100 transition-colors shrink-0 whitespace-nowrap"
              >
                {s.cta}
              </button>
            )}
          </li>
        ))}
      </ul>
    </div>
  )
}
