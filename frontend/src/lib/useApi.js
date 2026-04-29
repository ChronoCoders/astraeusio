import { useState, useEffect, useRef } from 'react'

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
