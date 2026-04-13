import { useEffect, useRef, useState } from 'react'
import { motion } from 'framer-motion'

function useInView(threshold = 0.3) {
  const ref = useRef<HTMLDivElement>(null)
  const [inView, setInView] = useState(false)
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const observer = new IntersectionObserver(
      ([entry]) => { if (entry.isIntersecting) { setInView(true); observer.disconnect() } },
      { threshold }
    )
    observer.observe(el)
    return () => observer.disconnect()
  }, [threshold])
  return { ref, inView }
}

function AnimatedNumber({ target, inView }: { target: number; inView: boolean }) {
  const [value, setValue] = useState(target)
  const hasAnimated = useRef(false)
  useEffect(() => {
    if (!inView || hasAnimated.current) return
    hasAnimated.current = true
    setValue(0)
    const start = performance.now()
    const duration = 1200
    const animate = (now: number) => {
      const progress = Math.min((now - start) / duration, 1)
      const eased = 1 - Math.pow(1 - progress, 3)
      setValue(Math.round(eased * target))
      if (progress < 1) requestAnimationFrame(animate)
    }
    requestAnimationFrame(animate)
  }, [inView, target])
  return <>{value}</>
}

export function SpeedSection() {
  const { ref, inView } = useInView()
  const mustardWidth = 8
  const remoteWidth = 100

  return (
    <section ref={ref} className="py-24 px-6">
      <div className="max-w-3xl mx-auto">
        {/* Big stat */}
        <div className="text-center mb-14">
          <motion.div initial={{ opacity: 1 }} animate={inView ? { opacity: 1 } : {}} transition={{ duration: 0.5 }}>
            <div className="mb-2">
              <span className="font-heading text-6xl sm:text-7xl md:text-8xl font-bold text-black tabular-nums tracking-tight">
                <AnimatedNumber target={43} inView={inView} />
                <span className="text-4xl sm:text-5xl md:text-6xl">ms</span>
              </span>
            </div>
            <p className="text-black/50 text-lg">
              Full tool-calling workflow &middot; in-process &middot; no network
            </p>
          </motion.div>
        </div>

        {/* Racing bars */}
        <div className="space-y-6 max-w-2xl mx-auto">
          <div>
            <div className="flex justify-between items-baseline mb-2">
              <span className="text-sm font-semibold text-black">MustardScript</span>
              <span className="text-sm font-mono font-bold text-black">43ms</span>
            </div>
            <div className="h-10 rounded-lg bg-black/8 overflow-hidden relative">
              <motion.div
                className="h-full rounded-lg"
                style={{ background: 'linear-gradient(90deg, #78350F, #92400E, #1C1917)' }}
                initial={{ width: `${mustardWidth}%` }}
                animate={{ width: `${mustardWidth}%` }}
                transition={{ duration: 0.6, ease: 'easeOut' }}
              />
              <motion.span
                className="absolute top-1/2 -translate-y-1/2 text-lg"
                style={{ left: `${mustardWidth + 1}%` }}
              >&#9889;</motion.span>
            </div>
          </div>

          <div>
            <div className="flex justify-between items-baseline mb-2">
              <span className="text-sm font-semibold text-black/50">Remote Sandbox</span>
              <span className="text-sm font-mono font-bold text-black/50">~1,000ms</span>
            </div>
            <div className="h-10 rounded-lg bg-black/8 overflow-hidden">
              <motion.div
                className="h-full rounded-lg bg-black/20"
                initial={{ width: `${remoteWidth}%` }}
                animate={{ width: `${remoteWidth}%` }}
                transition={{ duration: 2.5, ease: 'easeOut' }}
              />
            </div>
          </div>
        </div>

        <p className="text-center text-xs text-black/40 mt-8">
          4-tool orchestration workflow &middot; Apple M4 &middot; Node v24 &middot; Median of 50 runs
        </p>
      </div>
    </section>
  )
}
