import { useEffect, useState } from 'react'
import { useSearchParams, Link } from 'react-router-dom'
import Navbar from './Navbar'

export default function VerifyEmailPage({ onSignIn }) {
  const [params] = useSearchParams()
  const [status, setStatus] = useState('pending') // pending | success | error
  const [msg, setMsg]       = useState('')

  useEffect(() => {
    const token = params.get('token')
    if (!token) {
      setStatus('error')
      setMsg('No verification token found in URL.')
      return
    }

    fetch(`/auth/verify-email/${encodeURIComponent(token)}`, { method: 'POST' })
      .then(async r => {
        if (r.status === 204) {
          setStatus('success')
        } else {
          const d = await r.json().catch(() => ({}))
          setStatus('error')
          setMsg(d.error ?? 'Verification failed.')
        }
      })
      .catch(() => {
        setStatus('error')
        setMsg('Network error. Please try again.')
      })
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <Navbar onSignIn={onSignIn} />

      <div className="flex items-center justify-center min-h-screen px-6">
        <div className="max-w-sm w-full text-center">
          {status === 'pending' && (
            <>
              <div className="flex justify-center mb-4">
                <svg className="animate-spin h-6 w-6 text-zinc-400" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                  <path className="opacity-75" fill="currentColor"
                    d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                </svg>
              </div>
              <p className="text-zinc-400 text-sm font-mono">Verifying your email…</p>
            </>
          )}

          {status === 'success' && (
            <>
              <div className="text-green-400 text-4xl mb-4">✓</div>
              <h1 className="text-xl font-light text-zinc-100 mb-2">Email verified</h1>
              <p className="text-zinc-500 text-sm mb-6">Your email address has been confirmed.</p>
              <Link
                to="/"
                className="inline-block px-4 py-2 text-xs font-mono rounded bg-zinc-800 text-zinc-200 hover:bg-zinc-700 transition-colors"
              >
                Go to dashboard
              </Link>
            </>
          )}

          {status === 'error' && (
            <>
              <div className="text-red-400 text-4xl mb-4">✗</div>
              <h1 className="text-xl font-light text-zinc-100 mb-2">Verification failed</h1>
              <p className="text-zinc-500 text-sm mb-6">{msg || 'The link may have expired or already been used.'}</p>
              <Link
                to="/"
                className="inline-block px-4 py-2 text-xs font-mono rounded bg-zinc-800 text-zinc-200 hover:bg-zinc-700 transition-colors"
              >
                Back to home
              </Link>
            </>
          )}
        </div>
      </div>
    </div>
  )
}
