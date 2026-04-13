import { motion } from 'framer-motion'
import { InstallButton } from './InstallButton'

export function Hero() {
  return (
    <div className="relative">
      {/* Pill badge */}
      <motion.div
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4 }}
        className="inline-flex items-center gap-2 bg-black/8 rounded-full px-4 py-1.5 mb-8"
      >
        <span className="w-2 h-2 rounded-full bg-green-700" />
        <span className="text-sm font-medium text-black/70">Open source &middot; Apache-2.0</span>
      </motion.div>

      {/* Product name — two lines, biggest thing on the page */}
      <motion.h1
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, delay: 0.05 }}
        className="font-heading text-6xl sm:text-7xl font-bold leading-[1] tracking-tight mb-5"
      >
        <span className="text-black">Mustard</span><span className="text-black/50">Script</span>
      </motion.h1>

      {/* Tagline */}
      <motion.p
        initial={{ opacity: 0, y: 15 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, delay: 0.12 }}
        className="text-2xl sm:text-3xl text-black/65 mb-10 max-w-lg leading-snug font-medium"
      >
        Local JavaScript sandbox for AI agents to call tools
      </motion.p>

      {/* CTAs */}
      <motion.div
        initial={{ opacity: 0, y: 15 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, delay: 0.2 }}
        className="flex flex-col items-start gap-3 mb-12"
      >
        <InstallButton variant="dark" />
        <a
          href="https://github.com/keppoai/mustardscript"
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center justify-center gap-3 w-full max-w-sm px-7 py-4 rounded-2xl bg-black/8 hover:bg-black/15 text-black/80 hover:text-black transition-colors duration-200 font-bold text-base sm:text-lg whitespace-nowrap"
        >
          <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
            <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
          </svg>
          Star on GitHub
        </a>
      </motion.div>

      {/* Scroll hint */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 1.2, duration: 0.5 }}
        className="flex items-center gap-2 text-black/35 text-sm"
      >
        <motion.svg
          width="16" height="16" viewBox="0 0 16 16" fill="none"
          animate={{ y: [0, 4, 0] }}
          transition={{ duration: 1.5, repeat: Infinity, ease: 'easeInOut' }}
        >
          <path d="M3 6l5 5 5-5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </motion.svg>
        <span>Scroll to explore</span>
      </motion.div>
    </div>
  )
}
