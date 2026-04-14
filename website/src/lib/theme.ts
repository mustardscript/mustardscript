import { useEffect, useState, useCallback } from 'react'

export type ThemePreference = 'system' | 'light' | 'dark'
export type ResolvedTheme = 'light' | 'dark'

const STORAGE_KEY = 'mustard-theme'

function readStored(): ThemePreference {
  if (typeof localStorage === 'undefined') return 'system'
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored === 'light' || stored === 'dark' || stored === 'system') return stored
  return 'system'
}

function systemPrefersDark(): boolean {
  if (typeof window === 'undefined' || !window.matchMedia) return false
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

function resolve(pref: ThemePreference): ResolvedTheme {
  if (pref === 'system') return systemPrefersDark() ? 'dark' : 'light'
  return pref
}

function apply(resolved: ResolvedTheme) {
  if (typeof document === 'undefined') return
  const el = document.documentElement
  if (resolved === 'dark') el.classList.add('dark')
  else el.classList.remove('dark')
}

export function useTheme() {
  const [preference, setPreferenceState] = useState<ThemePreference>(() => readStored())
  const [resolved, setResolved] = useState<ResolvedTheme>(() => resolve(readStored()))

  useEffect(() => {
    const r = resolve(preference)
    setResolved(r)
    apply(r)
    if (typeof localStorage !== 'undefined') {
      if (preference === 'system') localStorage.removeItem(STORAGE_KEY)
      else localStorage.setItem(STORAGE_KEY, preference)
    }
  }, [preference])

  // Re-resolve when system preference changes (only matters for 'system' mode)
  useEffect(() => {
    if (preference !== 'system') return
    if (typeof window === 'undefined' || !window.matchMedia) return
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => {
      const r: ResolvedTheme = mq.matches ? 'dark' : 'light'
      setResolved(r)
      apply(r)
    }
    mq.addEventListener('change', onChange)
    return () => mq.removeEventListener('change', onChange)
  }, [preference])

  const setPreference = useCallback((p: ThemePreference) => {
    setPreferenceState(p)
  }, [])

  const cycle = useCallback(() => {
    setPreferenceState(prev => {
      if (prev === 'system') return 'light'
      if (prev === 'light') return 'dark'
      return 'system'
    })
  }, [])

  return { preference, resolved, setPreference, cycle }
}
