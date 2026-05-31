import { useState } from 'react'
import { useTranslation } from 'react-i18next'

const PLANS = [
  {
    id: 'free',
    features: ['Public API access', 'Basic Kp data'],
  },
  {
    id: 'starter',
    features: ['Dashboard access', 'Live space weather data', 'ISS tracking'],
  },
  {
    id: 'developer',
    features: ['API keys', 'ML Kp forecast', 'Anomaly detection', 'Email alerts', 'CSV export'],
  },
  {
    id: 'pro',
    features: ['Everything in Developer', 'Webhooks', 'Priority support'],
  },
  {
    id: 'business',
    features: ['Everything in Pro', 'Historical data access', 'Team seats'],
  },
  {
    id: 'enterprise',
    features: ['Everything in Business', 'SLA', 'Custom integrations'],
  },
]

const PLAN_COLOR = {
  free:       'border-zinc-700 text-zinc-400',
  starter:    'border-zinc-600 text-zinc-300',
  developer:  'border-blue-700 text-blue-400',
  pro:        'border-purple-700 text-purple-400',
  business:   'border-amber-700 text-amber-400',
  enterprise: 'border-orange-600 text-orange-400',
}

const LANGS = ['en', 'tr']

function authHeader() {
  return { Authorization: `Bearer ${localStorage.getItem('token')}` }
}

function Section({ title, children }) {
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded p-4 flex flex-col gap-3">
      <span className="text-zinc-500 text-xs font-mono uppercase tracking-widest">{title}</span>
      {children}
    </div>
  )
}

// ── Email Verification ────────────────────────────────────────────────────────

function EmailVerificationSection({ user }) {
  const { t } = useTranslation()
  const [sending, setSending] = useState(false)
  const [sent, setSent]       = useState(false)
  const [err, setErr]         = useState(null)

  async function resend() {
    setSending(true)
    setErr(null)
    setSent(false)
    try {
      const r = await fetch('/auth/resend-verification', {
        method: 'POST',
        headers: authHeader(),
      })
      if (r.status === 204) {
        setSent(true)
      } else {
        const d = await r.json().catch(() => ({}))
        setErr(d.error ?? t('auth.unknownError'))
      }
    } catch {
      setErr(t('auth.networkError'))
    } finally {
      setSending(false)
    }
  }

  if (user?.email_verified) {
    return (
      <Section title={t('settings.emailVerificationTitle')}>
        <div className="flex items-center gap-2">
          <span className="text-green-400 text-xs font-mono">{t('settings.emailVerified')}</span>
          <span className="text-zinc-600 text-xs font-mono">{user.email}</span>
        </div>
      </Section>
    )
  }

  return (
    <Section title={t('settings.emailVerificationTitle')}>
      <p className="text-yellow-400 text-xs font-mono">{t('settings.emailNotVerified')}</p>
      {err   && <p className="text-red-400 text-xs font-mono">{err}</p>}
      {sent  && <p className="text-green-400 text-xs font-mono">{t('settings.emailVerificationSent')}</p>}
      <button
        onClick={resend}
        disabled={sending}
        className="self-start px-4 py-2 text-xs font-mono rounded bg-zinc-700 text-zinc-100 hover:bg-zinc-600 disabled:bg-zinc-800 disabled:text-zinc-600 transition-colors"
      >
        {sending ? t('settings.sending') : t('settings.resendVerification')}
      </button>
    </Section>
  )
}

// ── 2FA ───────────────────────────────────────────────────────────────────────

function TwoFactorSection({ user, onUserChange }) {
  const { t } = useTranslation()
  const [step, setStep]       = useState('idle') // idle | setup | disable
  const [secret, setSecret]   = useState('')
  const [qrCode, setQrCode]   = useState('')
  const [code, setCode]       = useState('')
  const [loading, setLoading] = useState(false)
  const [err, setErr]         = useState(null)

  const enabled = user?.totp_enabled

  async function startSetup() {
    setLoading(true)
    setErr(null)
    try {
      const r = await fetch('/auth/2fa/setup', {
        method: 'POST',
        headers: authHeader(),
      })
      if (r.ok) {
        const d = await r.json()
        setSecret(d.secret)
        setQrCode(d.qr_code)
        setStep('setup')
      } else {
        const d = await r.json().catch(() => ({}))
        setErr(d.error ?? t('auth.unknownError'))
      }
    } catch {
      setErr(t('auth.networkError'))
    } finally {
      setLoading(false)
    }
  }

  async function confirmEnable() {
    setLoading(true)
    setErr(null)
    try {
      const r = await fetch('/auth/2fa/verify', {
        method: 'POST',
        headers: { ...authHeader(), 'Content-Type': 'application/json' },
        body: JSON.stringify({ code }),
      })
      if (r.status === 204) {
        setStep('idle')
        setCode('')
        setSecret('')
        setQrCode('')
        onUserChange?.({ ...user, totp_enabled: true })
      } else {
        const d = await r.json().catch(() => ({}))
        setErr(d.error ?? t('auth.unknownError'))
      }
    } catch {
      setErr(t('auth.networkError'))
    } finally {
      setLoading(false)
    }
  }

  async function confirmDisable() {
    setLoading(true)
    setErr(null)
    try {
      const r = await fetch('/auth/2fa/disable', {
        method: 'POST',
        headers: { ...authHeader(), 'Content-Type': 'application/json' },
        body: JSON.stringify({ code }),
      })
      if (r.status === 204) {
        setStep('idle')
        setCode('')
        onUserChange?.({ ...user, totp_enabled: false })
      } else {
        const d = await r.json().catch(() => ({}))
        setErr(d.error ?? t('auth.unknownError'))
      }
    } catch {
      setErr(t('auth.networkError'))
    } finally {
      setLoading(false)
    }
  }

  function cancel() { setStep('idle'); setCode(''); setErr(null); setSecret(''); setQrCode('') }

  return (
    <Section title={t('settings.twoFactorTitle')}>
      {/* Idle state */}
      {step === 'idle' && (
        <>
          {enabled
            ? <p className="text-green-400 text-xs font-mono">{t('settings.twoFactorEnabledMsg')}</p>
            : <p className="text-zinc-400 text-xs font-mono">{t('settings.twoFactorDesc')}</p>
          }
          {err && <p className="text-red-400 text-xs font-mono">{err}</p>}
          {enabled
            ? (
              <button onClick={() => { setStep('disable'); setErr(null) }}
                className="self-start px-4 py-2 text-xs font-mono rounded border border-zinc-700 text-zinc-400 hover:text-red-400 hover:border-red-800 transition-colors">
                {t('settings.twoFactorDisable')}
              </button>
            )
            : (
              <button onClick={startSetup} disabled={loading}
                className="self-start px-4 py-2 text-xs font-mono rounded bg-zinc-700 text-zinc-100 hover:bg-zinc-600 disabled:bg-zinc-800 disabled:text-zinc-600 transition-colors">
                {loading ? t('common.loading') : t('settings.twoFactorEnable')}
              </button>
            )
          }
        </>
      )}

      {/* Setup step - show QR + secret */}
      {step === 'setup' && (
        <div className="flex flex-col gap-4">
          <p className="text-zinc-400 text-xs">{t('settings.twoFactorSetupInstructions')}</p>
          {qrCode && (
            <div className="self-start bg-white p-2 rounded">
              <img src={qrCode} alt="2FA QR code" className="w-40 h-40" />
            </div>
          )}
          <div className="flex flex-col gap-1">
            <span className="text-zinc-600 text-xs font-mono">{t('settings.manualEntryKey')}</span>
            <code className="text-zinc-300 text-xs font-mono bg-zinc-950 rounded px-3 py-2 break-all">{secret}</code>
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">{t('settings.confirmationCode')}</label>
            <input
              type="text"
              name="totp-confirm-code"
              inputMode="numeric"
              maxLength={6}
              value={code}
              onChange={e => setCode(e.target.value.replace(/\D/g, ''))}
              placeholder="000000"
              className="bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500 tracking-widest w-32"
            />
          </div>
          {err && <p className="text-red-400 text-xs font-mono">{err}</p>}
          <div className="flex gap-2">
            <button onClick={confirmEnable} disabled={loading || code.length !== 6}
              className="px-4 py-2 text-xs font-mono rounded bg-zinc-700 text-zinc-100 hover:bg-zinc-600 disabled:bg-zinc-800 disabled:text-zinc-600 transition-colors">
              {loading ? t('settings.verifying') : t('settings.confirmEnable')}
            </button>
            <button onClick={cancel} className="px-4 py-2 text-xs font-mono rounded border border-zinc-700 text-zinc-500 hover:text-zinc-300 transition-colors">
              {t('settings.cancel')}
            </button>
          </div>
        </div>
      )}

      {/* Disable step */}
      {step === 'disable' && (
        <div className="flex flex-col gap-3">
          <p className="text-zinc-400 text-xs">{t('settings.enterCodeToDisable')}</p>
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">{t('settings.authenticatorCode')}</label>
            <input
              type="text"
              name="totp-disable-code"
              inputMode="numeric"
              maxLength={6}
              value={code}
              onChange={e => setCode(e.target.value.replace(/\D/g, ''))}
              placeholder="000000"
              className="bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500 tracking-widest w-32"
            />
          </div>
          {err && <p className="text-red-400 text-xs font-mono">{err}</p>}
          <div className="flex gap-2">
            <button onClick={confirmDisable} disabled={loading || code.length !== 6}
              className="px-4 py-2 text-xs font-mono rounded border border-zinc-700 text-red-400 hover:bg-red-950/30 disabled:opacity-50 transition-colors">
              {loading ? t('settings.disabling') : t('settings.twoFactorDisable')}
            </button>
            <button onClick={cancel} className="px-4 py-2 text-xs font-mono rounded border border-zinc-700 text-zinc-500 hover:text-zinc-300 transition-colors">
              {t('settings.cancel')}
            </button>
          </div>
        </div>
      )}
    </Section>
  )
}

// ── Plan ──────────────────────────────────────────────────────────────────────

function PlanSection({ user, onUserChange }) {
  const { t } = useTranslation()
  const plan = user?.plan ?? 'starter'
  const [open, setOpen]       = useState(false)
  const [selected, setSelected] = useState(plan)
  const [saving, setSaving]   = useState(false)
  const [err, setErr]         = useState(null)

  async function handleSave() {
    if (selected === plan) { setOpen(false); return }
    setSaving(true)
    setErr(null)
    try {
      const r = await fetch('/api/user/plan', {
        method: 'POST',
        headers: { ...authHeader(), 'Content-Type': 'application/json' },
        body: JSON.stringify({ plan: selected }),
      })
      if (r.status === 204) {
        onUserChange?.({ ...user, plan: selected })
        setOpen(false)
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

  const current = PLANS.find(p => p.id === plan) ?? PLANS[1]
  const planCls = PLAN_COLOR[plan] ?? PLAN_COLOR.starter

  return (
    <Section title={t('settings.planTitle')}>
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className={`text-xs font-mono border rounded px-2 py-0.5 ${planCls}`}>
            {t(`plan.${plan}`)}
          </span>
          <span className="text-zinc-500 text-xs font-mono">{t('settings.currentPlan')}</span>
        </div>
        <button
          onClick={() => { setSelected(plan); setErr(null); setOpen(true) }}
          className="px-3 py-1.5 text-xs font-mono rounded border border-zinc-700 text-zinc-400 hover:text-zinc-200 hover:border-zinc-500 transition-colors"
        >
          {t('settings.changePlan')}
        </button>
      </div>

      <div className="flex flex-col gap-1">
        {current.features.map(f => (
          <span key={f} className="text-zinc-500 text-xs font-mono">· {f}</span>
        ))}
      </div>

      {/* Modal */}
      {open && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm" onClick={() => setOpen(false)}>
          <div className="bg-zinc-900 border border-zinc-700 rounded-xl p-6 w-full max-w-sm mx-4 flex flex-col gap-4" onClick={e => e.stopPropagation()}>
            <div className="flex items-center justify-between">
              <span className="text-zinc-100 text-sm font-mono">{t('settings.selectPlan')}</span>
              <button onClick={() => setOpen(false)} className="text-zinc-600 hover:text-zinc-300 transition-colors text-lg leading-none">×</button>
            </div>

            <div className="flex flex-col gap-2">
              {PLANS.map(p => {
                const cls = PLAN_COLOR[p.id] ?? PLAN_COLOR.starter
                const isSelected = selected === p.id
                return (
                  <button
                    key={p.id}
                    onClick={() => setSelected(p.id)}
                    className={[
                      'flex items-center gap-3 px-3 py-2.5 rounded-lg border text-left transition-colors',
                      isSelected
                        ? 'border-zinc-400 bg-zinc-800'
                        : 'border-zinc-800 hover:border-zinc-600',
                    ].join(' ')}
                  >
                    <span className={`text-xs font-mono border rounded px-1.5 py-0.5 shrink-0 ${cls}`}>
                      {t(`plan.${p.id}`)}
                    </span>
                    <span className="text-zinc-400 text-xs">{p.features[0]}{p.features.length > 1 ? ` +${p.features.length - 1} more` : ''}</span>
                    {isSelected && <span className="ml-auto text-zinc-300 text-xs">✓</span>}
                  </button>
                )
              })}
            </div>

            {err && <p className="text-red-400 text-xs font-mono">{err}</p>}

            <div className="flex gap-2">
              <button
                onClick={handleSave}
                disabled={saving}
                className="flex-1 py-2 text-xs font-mono rounded bg-zinc-100 text-zinc-950 hover:bg-white disabled:bg-zinc-700 disabled:text-zinc-500 transition-colors"
              >
                {saving ? t('common.loading') : t('settings.savePlan')}
              </button>
              <button
                onClick={() => setOpen(false)}
                className="px-4 py-2 text-xs font-mono rounded border border-zinc-700 text-zinc-500 hover:text-zinc-300 transition-colors"
              >
                {t('settings.cancel')}
              </button>
            </div>
          </div>
        </div>
      )}
    </Section>
  )
}

// ── Main ──────────────────────────────────────────────────────────────────────

export default function SettingsPage({ onLogout, user, onUserChange }) {
  const { t, i18n } = useTranslation()

  const [current, setCurrent]       = useState('')
  const [newPw, setNewPw]           = useState('')
  const [confirm, setConfirm]       = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [pwError, setPwError]       = useState(null)
  const [pwSuccess, setPwSuccess]   = useState(false)

  async function handleChangePassword(e) {
    e.preventDefault()
    setPwError(null)
    setPwSuccess(false)

    if (newPw !== confirm) {
      setPwError(t('auth.passwordMismatch'))
      return
    }
    if (newPw.length < 8) {
      setPwError(t('settings.pwTooShort'))
      return
    }

    setSubmitting(true)
    try {
      const r = await fetch('/auth/change-password', {
        method: 'POST',
        headers: { ...authHeader(), 'Content-Type': 'application/json' },
        body: JSON.stringify({ current_password: current, new_password: newPw }),
      })
      if (r.status === 204) {
        setPwSuccess(true)
        setCurrent('')
        setNewPw('')
        setConfirm('')
      } else {
        const d = await r.json().catch(() => ({}))
        setPwError(d.error ?? t('auth.unknownError'))
      }
    } catch {
      setPwError(t('auth.networkError'))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="flex flex-col gap-6 max-w-lg">

      {/* ── Email verification ───────────────────────────────────────── */}
      <EmailVerificationSection user={user} />

      {/* ── Plan ─────────────────────────────────────────────────────── */}
      <PlanSection user={user} onUserChange={onUserChange} />

      {/* ── 2FA ─────────────────────────────────────────────────────── */}
      <TwoFactorSection user={user} onUserChange={onUserChange} />

      {/* ── Language ────────────────────────────────────────────────── */}
      <Section title={t('settings.languageTitle')}>
        <div className="flex items-center gap-2">
          {LANGS.map(lng => (
            <button
              key={lng}
              onClick={() => i18n.changeLanguage(lng)}
              className={[
                'px-4 py-1.5 text-xs font-mono rounded border transition-colors',
                i18n.language === lng
                  ? 'bg-zinc-700 border-zinc-600 text-zinc-100'
                  : 'border-zinc-700 text-zinc-500 hover:text-zinc-300 hover:border-zinc-500',
              ].join(' ')}
            >
              {lng.toUpperCase()}
            </button>
          ))}
          <span className="text-zinc-600 text-xs font-mono ml-2">
            {i18n.language === 'tr' ? 'Türkçe' : 'English'}
          </span>
        </div>
      </Section>

      {/* ── Change password ──────────────────────────────────────────── */}
      <Section title={t('settings.changePasswordTitle')}>
        <form onSubmit={handleChangePassword} className="flex flex-col gap-3">
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">{t('settings.currentPassword')}</label>
            <input type="password" name="current-password" value={current} onChange={e => setCurrent(e.target.value)} required
              className="bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500" />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">{t('settings.newPassword')}</label>
            <input type="password" name="new-password" value={newPw} onChange={e => setNewPw(e.target.value)} required
              className="bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500" />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">{t('settings.confirmNewPassword')}</label>
            <input type="password" name="confirm-new-password" value={confirm} onChange={e => setConfirm(e.target.value)} required
              className="bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500" />
          </div>
          {pwError   && <p className="text-red-400 text-xs font-mono">{pwError}</p>}
          {pwSuccess && <p className="text-green-400 text-xs font-mono">{t('settings.pwChanged')}</p>}
          <button type="submit" disabled={submitting}
            className="self-start px-4 py-2 text-xs font-mono rounded bg-zinc-700 text-zinc-100 hover:bg-zinc-600 disabled:bg-zinc-800 disabled:text-zinc-600 transition-colors">
            {submitting ? t('common.loading') : t('settings.changePasswordBtn')}
          </button>
        </form>
      </Section>

      {/* ── Sign out ─────────────────────────────────────────────────── */}
      <Section title={t('settings.sessionTitle')}>
        <button onClick={onLogout}
          className="self-start px-4 py-2 text-xs font-mono rounded border border-zinc-700 text-zinc-400 hover:text-red-400 hover:border-red-800 transition-colors">
          {t('auth.logout')}
        </button>
      </Section>

    </div>
  )
}
