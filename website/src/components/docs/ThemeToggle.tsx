import { useTheme, type ThemePreference } from '../../lib/theme'

function SunIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <circle cx="10" cy="10" r="3.25" />
      <path d="M10 2v2M10 16v2M2 10h2M16 10h2M4.2 4.2l1.4 1.4M14.4 14.4l1.4 1.4M4.2 15.8l1.4-1.4M14.4 5.6l1.4-1.4" />
    </svg>
  )
}

function MoonIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
      <path d="M16 11.5A7 7 0 0 1 8.5 4a7 7 0 1 0 7.5 7.5z" />
    </svg>
  )
}

function AutoIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="10" cy="10" r="7" />
      <path d="M10 3v14" />
      <path d="M10 3a7 7 0 0 0 0 14z" fill="currentColor" stroke="none" />
    </svg>
  )
}

const LABELS: Record<ThemePreference, string> = {
  system: 'System theme',
  light: 'Light theme',
  dark: 'Dark theme',
}

export function ThemeToggle() {
  const { preference, cycle } = useTheme()

  const icon =
    preference === 'light' ? <SunIcon /> :
    preference === 'dark' ? <MoonIcon /> :
    <AutoIcon />

  return (
    <button
      onClick={cycle}
      className="p-1.5 rounded-lg hover:bg-black/5 dark:hover:bg-white/10 transition-colors text-black/60 hover:text-black dark:text-white/60 dark:hover:text-white"
      aria-label={`${LABELS[preference]} — click to change`}
      title={LABELS[preference]}
    >
      {icon}
    </button>
  )
}
