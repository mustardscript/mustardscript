import { useState } from 'react'
import { motion } from 'framer-motion'

export function InstallButton({ variant = 'brand' }: { variant?: 'brand' | 'dark' }) {
  const [copied, setCopied] = useState(false)

  const handleCopy = async () => {
    await navigator.clipboard.writeText('npm install mustardscript')
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const isDark = variant === 'dark'

  return (
    <button
      onClick={handleCopy}
      className={`group flex items-center justify-center gap-4 w-full max-w-sm font-mono font-bold text-base sm:text-lg rounded-2xl px-7 py-4 transition-colors duration-200 cursor-pointer ${
        isDark
          ? 'bg-black text-white shadow-lg shadow-black/20 hover:bg-black/90'
          : 'bg-mustard hover:bg-mustard-light text-black'
      }`}
      style={isDark ? undefined : { boxShadow: '0 0 30px var(--color-cta-glow), 0 4px 20px rgba(0,0,0,0.3)' }}
    >
      <span className={`select-none text-xl ${isDark ? 'text-white/40' : 'text-black/40'}`}>$</span>
      <span>npm install mustardscript</span>
      <div className="ml-1 relative flex items-center">
        {copied ? (
          <motion.svg
            initial={{ scale: 0, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            className={`w-5 h-5 ${isDark ? 'text-green-400' : 'text-green-700'}`}
            fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
          </motion.svg>
        ) : (
          <svg
            className={`w-5 h-5 transition-colors ${
              isDark ? 'text-white/30 group-hover:text-white/60' : 'text-black/30 group-hover:text-black/60'
            }`}
            fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}
          >
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
            <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" />
          </svg>
        )}
      </div>
    </button>
  )
}
