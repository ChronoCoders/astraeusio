import { useState, useEffect, useRef } from 'react'

export function authedFetch(url, opts = {}) {
  const token = localStorage.getItem('token')
  return fetch(url, {
    ...opts,
    headers: {
      ...(opts.headers ?? {}),
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
  })
}

// A sentence for a backend error code.
//
// Every page that creates something renders `d.error` straight from the JSON,
// so a blocked user was shown `plan_required` or `email_verification_required`
// verbatim. Those are wire codes, not messages. Mapped in one place because the
// same codes come back from five different endpoints, and a mapping kept per
// page is a mapping that goes stale on four of them.
//
// An unmapped code falls back to the caller's own message rather than being
// shown raw: a code nobody has written words for is not words.
export function apiError(payload, t, fallbackKey) {
  const key = {
    email_verification_required: 'errors.emailVerificationRequired',
    verification_email_failed:   'errors.verificationEmailFailed',
    plan_required:               'errors.planRequired',
  }[payload?.error]
  return key ? t(key) : t(fallbackKey)
}

export function useApi(url, intervalMs = 60000) {
  const [data, setData]       = useState(null)
  const [loading, setLoading] = useState(true)
  const [error, setError]     = useState(null)
  const timer = useRef(null)

  useEffect(() => {
    let cancelled = false

    async function run() {
      let planGated = false
      const ctrl = new AbortController()
      const timeout = setTimeout(() => ctrl.abort(), 90000)
      try {
        const token = localStorage.getItem('token')
        const res = await fetch(url, {
          signal: ctrl.signal,
          headers: token ? { Authorization: `Bearer ${token}` } : {},
        })
        if (res.status === 403) {
          planGated = true
          if (!cancelled) { setError('HTTP 403'); setLoading(false) }
          return
        }
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        const json = await res.json()
        if (!cancelled) { setData(json); setError(null) }
      } catch (e) {
        if (!cancelled && e.name !== 'AbortError') setError(e.message)
      } finally {
        clearTimeout(timeout)
        if (!cancelled && !planGated) {
          setLoading(false)
          timer.current = setTimeout(run, intervalMs)
        }
      }
    }

    run()
    return () => {
      cancelled = true
      clearTimeout(timer.current)
    }
  }, [url, intervalMs])

  return { data, loading, error }
}
