import { motion } from 'framer-motion'

export function MustardBottle({ className = '' }: { className?: string }) {
  return (
    <motion.svg
      viewBox="0 0 120 200"
      fill="none"
      className={className}
      whileHover={{ rotate: [-8, 8, -4, 0], transition: { duration: 0.5 } }}
    >
      {/* Cap */}
      <rect x="42" y="8" width="36" height="20" rx="4" fill="#D97706" />
      <rect x="48" y="0" width="24" height="12" rx="6" fill="#B45309" />

      {/* Nozzle */}
      <path d="M50 28 L50 42 Q50 48 54 48 L66 48 Q70 48 70 42 L70 28" fill="#FBBF24" />
      <ellipse cx="60" cy="48" rx="6" ry="3" fill="#D97706" />

      {/* Squeeze drip from nozzle */}
      <motion.ellipse
        cx="60" cy="54" rx="3" ry="4"
        fill="#E8B931"
        animate={{ cy: [54, 62, 54], ry: [4, 6, 4], opacity: [0.9, 1, 0.9] }}
        transition={{ duration: 2.5, repeat: Infinity, ease: 'easeInOut' }}
      />

      {/* Body */}
      <path
        d="M38 48 Q28 60 26 90 Q24 140 30 165 Q34 180 50 185 L70 185 Q86 180 90 165 Q96 140 94 90 Q92 60 82 48 Z"
        fill="#FBBF24"
      />

      {/* Body highlight */}
      <path
        d="M44 55 Q36 68 34 90 Q33 125 36 155 Q38 168 48 172 L52 172 Q42 168 40 155 Q37 125 38 90 Q39 68 46 55 Z"
        fill="#FCD34D"
        opacity="0.6"
      />

      {/* Label area */}
      <rect x="34" y="90" width="52" height="55" rx="6" fill="#92400E" opacity="0.9" />
      <rect x="38" y="94" width="44" height="47" rx="4" fill="#78350F" />

      {/* Label text - "M" */}
      <text x="60" y="125" textAnchor="middle" fill="#FBBF24" fontSize="28" fontWeight="900" fontFamily="sans-serif">
        M
      </text>

      {/* Label accent line */}
      <rect x="44" y="132" width="32" height="2" rx="1" fill="#D97706" opacity="0.6" />

      {/* Body shadow/contour */}
      <path
        d="M82 48 Q92 60 94 90 Q96 140 90 165 Q86 180 70 185"
        stroke="#B45309"
        strokeWidth="1.5"
        fill="none"
        opacity="0.4"
      />

      {/* Squeeze lines (showing it's being squeezed) */}
      <motion.g
        animate={{ opacity: [0, 0.4, 0] }}
        transition={{ duration: 2.5, repeat: Infinity, ease: 'easeInOut' }}
      >
        <path d="M28 100 Q24 110 28 120" stroke="#D97706" strokeWidth="1.5" fill="none" />
        <path d="M92 100 Q96 110 92 120" stroke="#D97706" strokeWidth="1.5" fill="none" />
      </motion.g>
    </motion.svg>
  )
}
