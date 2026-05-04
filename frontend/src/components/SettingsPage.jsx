import { useState } from 'react'
import { useTranslation } from 'react-i18next'

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
        setErr(d.error ?? 'Failed to send verification email.')
      }
    } catch {
      setErr('Network error.')
    } finally {
      setSending(false)
    }
  }

  if (user?.email_verified) {
    return (
      <Section title="Email verification">
        <div className="flex items-center gap-2">
          <span className="text-green-400 text-xs font-mono">✓ Verified</span>
          <span className="text-zinc-600 text-xs font-mono">{user.email}</span>
        </div>
      </Section>
    )
  }

  return (
    <Section title="Email verification">
      <p className="text-yellow-400 text-xs font-mono">⚠ Your email address is not verified.</p>
      {err   && <p className="text-red-400 text-xs font-mono">{err}</p>}
      {sent  && <p className="text-green-400 text-xs font-mono">Verification email sent — check your inbox.</p>}
      <button
        onClick={resend}
        disabled={sending}
        className="self-start px-4 py-2 text-xs font-mono rounded bg-zinc-700 text-zinc-100 hover:bg-zinc-600 disabled:bg-zinc-800 disabled:text-zinc-600 transition-colors"
      >
        {sending ? 'Sending…' : 'Resend verification email'}
      </button>
    </Section>
  )
}

// ── 2FA ───────────────────────────────────────────────────────────────────────

function TwoFactorSection({ user, onUserChange }) {
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
        setErr(d.error ?? 'Failed to start 2FA setup.')
      }
    } catch {
      setErr('Network error.')
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
        setErr(d.error ?? 'Verification failed.')
      }
    } catch {
      setErr('Network error.')
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
        setErr(d.error ?? 'Invalid code.')
      }
    } catch {
      setErr('Network error.')
    } finally {
      setLoading(false)
    }
  }

  function cancel() { setStep('idle'); setCode(''); setErr(null); setSecret(''); setQrCode('') }

  return (
    <Section title="Two-factor authentication (TOTP)">
      {/* Idle state */}
      {step === 'idle' && (
        <>
          {enabled
            ? <p className="text-green-400 text-xs font-mono">✓ 2FA is enabled on your account.</p>
            : <p className="text-zinc-400 text-xs font-mono">Add an extra layer of security with a TOTP authenticator app.</p>
          }
          {err && <p className="text-red-400 text-xs font-mono">{err}</p>}
          {enabled
            ? (
              <button onClick={() => { setStep('disable'); setErr(null) }}
                className="self-start px-4 py-2 text-xs font-mono rounded border border-zinc-700 text-zinc-400 hover:text-red-400 hover:border-red-800 transition-colors">
                Disable 2FA
              </button>
            )
            : (
              <button onClick={startSetup} disabled={loading}
                className="self-start px-4 py-2 text-xs font-mono rounded bg-zinc-700 text-zinc-100 hover:bg-zinc-600 disabled:bg-zinc-800 disabled:text-zinc-600 transition-colors">
                {loading ? 'Loading…' : 'Enable 2FA'}
              </button>
            )
          }
        </>
      )}

      {/* Setup step — show QR + secret */}
      {step === 'setup' && (
        <div className="flex flex-col gap-4">
          <p className="text-zinc-400 text-xs">
            Scan this QR code with your authenticator app (Google Authenticator, Authy, etc.), then enter the 6-digit code to confirm.
          </p>
          {qrCode && (
            <div className="self-start bg-white p-2 rounded">
              <img src={qrCode} alt="2FA QR code" className="w-40 h-40" />
            </div>
          )}
          <div className="flex flex-col gap-1">
            <span className="text-zinc-600 text-xs font-mono">Manual entry key</span>
            <code className="text-zinc-300 text-xs font-mono bg-zinc-950 rounded px-3 py-2 break-all">{secret}</code>
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">Confirmation code</label>
            <input
              type="text"
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
              {loading ? 'Verifying…' : 'Confirm & enable'}
            </button>
            <button onClick={cancel} className="px-4 py-2 text-xs font-mono rounded border border-zinc-700 text-zinc-500 hover:text-zinc-300 transition-colors">
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* Disable step */}
      {step === 'disable' && (
        <div className="flex flex-col gap-3">
          <p className="text-zinc-400 text-xs">Enter your current authenticator code to disable 2FA.</p>
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">Authenticator code</label>
            <input
              type="text"
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
              {loading ? 'Disabling…' : 'Disable 2FA'}
            </button>
            <button onClick={cancel} className="px-4 py-2 text-xs font-mono rounded border border-zinc-700 text-zinc-500 hover:text-zinc-300 transition-colors">
              Cancel
            </button>
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
            <input type="password" value={current} onChange={e => setCurrent(e.target.value)} required
              className="bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500" />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">{t('settings.newPassword')}</label>
            <input type="password" value={newPw} onChange={e => setNewPw(e.target.value)} required
              className="bg-zinc-950 border border-zinc-700 rounded px-3 py-2 text-sm text-zinc-200 font-mono focus:outline-none focus:border-zinc-500" />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-zinc-600 text-xs font-mono">{t('settings.confirmNewPassword')}</label>
            <input type="password" value={confirm} onChange={e => setConfirm(e.target.value)} required
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
