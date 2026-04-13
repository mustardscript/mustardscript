import { useState, useEffect, useRef, useCallback } from 'react'
import { motion } from 'framer-motion'
import { highlightLine } from './highlight'
import { Hero } from './Hero'

// ── Code data ───────────────────────────────────────────────────────

const FULL_LINES = [
  '// Host setup — you write this in Node.js',
  'const program = new Mustard(`',
  '  const account = load_account(accountId);',
  '  const policy  = lookup_plan_policy(account.plan, seats);',
  '  const quote   = create_quote({',
  '    accountId:   account.id,',
  '    targetPlan:  policy.targetPlan,',
  '    seats,',
  '    monthlyDelta: policy.monthlyDelta,',
  '  });',
  '  ({',
  '    quoteId:      quote.quoteId,',
  '    approvalMode: policy.requiresApproval',
  '                    ? "manual" : "automatic",',
  '  });',
  '`);',
  '',
  'const result = await program.run({',
  '  inputs: { accountId: "acct_91", seats: 25 },',
  '  capabilities: {',
  '    load_account,',
  '    lookup_plan_policy,',
  '    create_quote,',
  '  },',
  '});',
]

const GUEST_LINES = [
  '// Sandboxed guest code — the part AI writes',
  'const account = load_account(accountId);',
  'const policy  = lookup_plan_policy(account.plan, seats);',
  'const quote   = create_quote({',
  '  accountId:    account.id,',
  '  targetPlan:   policy.targetPlan,',
  '  seats,',
  '  monthlyDelta: policy.monthlyDelta,',
  '});',
  '',
  '({',
  '  quoteId:      quote.quoteId,',
  '  approvalMode: policy.requiresApproval',
  '                  ? "manual" : "automatic",',
  '});',
]

const GUEST_START = 2
const GUEST_END = 14

// Step 0 = hero, 1 = highlight guest, 2 = guest only
type Step = 0 | 1 | 2

const ANNOTATION_STEPS = [
  {
    num: '01',
    title: 'AI writes the guest code',
    body: 'Everything inside the template literal is sandboxed guest code — that\'s the only part the AI generates. A small, constrained subset of JavaScript that calls your tools by name.',
  },
  {
    num: '02',
    title: 'Runs in-process',
    body: 'MustardScript executes the guest code inside your Node.js process. No remote sandbox, no network round-trip, no cold start. Just fast, safe execution.',
  },
]

// ── Splotch backgrounds ─────────────────────────────────────────────

function Splotch1({ opacity }: { opacity: number }) {
  return (
    <svg viewBox="0 0 500 400" className="absolute inset-0 w-full h-full pointer-events-none" preserveAspectRatio="none">
      <path d="M80,50 C100,20 160,5 220,15 C260,22 290,8 340,25 C380,38 430,55 450,100 C465,135 460,170 445,210 C435,240 450,270 430,310 C415,340 380,365 330,375 C280,385 240,370 190,365 C140,360 100,380 65,350 C35,325 15,280 20,230 C24,190 10,160 25,120 C38,85 55,75 80,50Z" fill="#F5D563" fillOpacity={opacity} />
      <ellipse cx="455" cy="75" rx="18" ry="14" fill="#F5D563" fillOpacity={opacity} transform="rotate(-20 455 75)" />
      <circle cx="42" cy="340" r="10" fill="#F5D563" fillOpacity={opacity} />
      <circle cx="350" cy="390" r="7" fill="#F5D563" fillOpacity={opacity} />
      <circle cx="90" cy="25" r="8" fill="#F5D563" fillOpacity={opacity} />
    </svg>
  )
}

function Splotch2({ opacity }: { opacity: number }) {
  return (
    <svg viewBox="0 0 500 400" className="absolute inset-0 w-full h-full pointer-events-none" preserveAspectRatio="none">
      <path d="M60,70 C90,25 150,10 210,20 C250,28 310,5 370,30 C410,48 445,85 455,130 C462,165 470,200 450,245 C430,285 440,320 400,355 C360,380 310,390 250,380 C195,372 150,390 100,360 C55,335 25,295 20,245 C16,200 30,165 28,125 C26,90 35,100 60,70Z" fill="#F5D563" fillOpacity={opacity} />
      <circle cx="25" cy="180" r="12" fill="#F5D563" fillOpacity={opacity} />
      <circle cx="480" cy="260" r="9" fill="#F5D563" fillOpacity={opacity} />
      <ellipse cx="130" cy="390" rx="14" ry="8" fill="#F5D563" fillOpacity={opacity} transform="rotate(-10 130 390)" />
    </svg>
  )
}

const SPLOTCH_COMPONENTS = [Splotch1, Splotch2]

function SplotchCard({ idx, active, children }: { idx: number; active: boolean; children: React.ReactNode }) {
  const SplotchSvg = SPLOTCH_COMPONENTS[idx]
  const opacity = active ? 0.6 : 0.3
  return (
    <div className={`relative transition-all duration-500 ${active ? 'opacity-100' : 'opacity-40'}`}>
      {/* Clip overflow so splotch doesn't escape viewport */}
      <div className="absolute -inset-[12%] pointer-events-none overflow-hidden">
        <SplotchSvg opacity={opacity} />
      </div>
      <div className="relative px-8 py-8">
        {children}
      </div>
    </div>
  )
}

// ── Scroll step detection ───────────────────────────────────────────

function useScrollStep(refs: React.RefObject<HTMLDivElement | null>[]): Step {
  const [step, setStep] = useState<Step>(0)

  const update = useCallback(() => {
    const viewMid = window.innerHeight / 2
    let closest = 0
    let closestDist = Infinity
    refs.forEach((ref, idx) => {
      if (!ref.current) return
      const rect = ref.current.getBoundingClientRect()
      const center = rect.top + rect.height / 2
      const dist = Math.abs(center - viewMid)
      if (dist < closestDist) { closestDist = dist; closest = idx }
    })
    setStep(closest as Step)
  }, [refs])

  useEffect(() => {
    const frame = window.requestAnimationFrame(update)
    window.addEventListener('scroll', update, { passive: true })
    return () => {
      window.cancelAnimationFrame(frame)
      window.removeEventListener('scroll', update)
    }
  }, [update])

  return step
}

// ── Code block ──────────────────────────────────────────────────────

function CodeBlock({ step }: { step: Step }) {
  // step 0 = full program, step 1 = highlight guest lines, step 2 = guest only
  const showGuest = step === 2
  const lines = showGuest ? GUEST_LINES : FULL_LINES
  const filename = showGuest ? 'guest-code.js' : 'example.ts'

  return (
    <div className="code-block shadow-2xl shadow-black/30 w-full relative" style={{ transform: 'none' }}>
      <div className="flex items-center gap-1.5 px-4 py-2 border-b border-amber-900/30 bg-black/20">
        <div className="w-2 h-2 rounded-full bg-[#ff5f57]" />
        <div className="w-2 h-2 rounded-full bg-[#febc2e]" />
        <div className="w-2 h-2 rounded-full bg-[#28c840]" />
        <motion.span key={filename} initial={{ opacity: 0 }} animate={{ opacity: 1 }}
          className="ml-2.5 text-xs font-mono text-amber-700/60">{filename}</motion.span>
      </div>

      <pre className={`px-6 py-5 font-mono overflow-x-auto text-[#D4C8A8] transition-all duration-500 ${
        showGuest ? 'text-[0.9375rem] leading-[1.85]' : 'text-[0.8125rem] leading-[1.75]'
      }`}>
        <code>
          {lines.map((line, i) => {
            const isGuest = !showGuest && i >= GUEST_START && i <= GUEST_END
            const dimmed = step === 1 && !showGuest && !isGuest
            return (
              <div key={`${step}-${i}`} className="flex transition-all duration-700 ease-out"
                style={{ opacity: dimmed ? 0.35 : 1 }}>
                {isGuest && step === 1 && (
                  <motion.div initial={{ scaleY: 0 }} animate={{ scaleY: 1 }}
                    transition={{ duration: 0.3, delay: i * 0.015 }}
                    className="w-[3px] mr-3 rounded-full bg-[#FBBF24] flex-shrink-0 origin-top" />
                )}
                <span dangerouslySetInnerHTML={{ __html: highlightLine(line) }} />
              </div>
            )
          })}
        </code>
      </pre>

      {step >= 1 && (
        <motion.div key={step} initial={{ opacity: 0, x: 8 }} animate={{ opacity: 1, x: 0 }}
          className="absolute top-9 right-4 bg-[#FBBF24]/20 border border-[#FBBF24]/40 text-[#FBBF24] text-xs font-mono font-semibold px-3 py-1 rounded-full">
          {step === 1 ? 'AI writes this' : 'guest code'}
        </motion.div>
      )}
    </div>
  )
}

// ── Unified section ─────────────────────────────────────────────────

export function CodeStorytelling() {
  const heroRef = useRef<HTMLDivElement>(null)
  const ref1 = useRef<HTMLDivElement>(null)
  const ref2 = useRef<HTMLDivElement>(null)
  const step = useScrollStep([heroRef, ref1, ref2])

  return (
    <section className="relative px-6 bg-mustard">
      {/* Subtle texture */}
      <div
        className="absolute inset-0 opacity-[0.04] pointer-events-none"
        style={{
          backgroundImage: 'radial-gradient(circle at 1px 1px, #000 1px, transparent 0)',
          backgroundSize: '32px 32px',
        }}
      />

      {/* Desktop: unified layout */}
      <div className="hidden md:flex max-w-7xl mx-auto gap-12 relative">
        {/* Left column */}
        <div className="w-[46%] flex-shrink-0">
          {/* Hero — vertically centered, compact */}
          <div ref={heroRef} className="h-screen flex items-center pt-12 pb-8">
            <Hero />
          </div>

          {/* Step 1 — bigger headings, tighter height */}
          <div ref={ref1} className="h-[70vh] flex items-center">
            <SplotchCard idx={0} active={step === 1}>
              <span className="font-heading text-black/20 text-5xl font-bold mb-3 block leading-none">01</span>
              <h3 className="font-heading text-3xl md:text-[2.5rem] font-bold text-black mb-5 leading-snug">{ANNOTATION_STEPS[0].title}</h3>
              <p className="text-black/65 leading-relaxed text-[1.05rem]">{ANNOTATION_STEPS[0].body}</p>
            </SplotchCard>
          </div>

          {/* Step 2 */}
          <div ref={ref2} className="h-[70vh] flex items-center">
            <SplotchCard idx={1} active={step === 2}>
              <span className="font-heading text-black/20 text-5xl font-bold mb-3 block leading-none">02</span>
              <h3 className="font-heading text-3xl md:text-[2.5rem] font-bold text-black mb-5 leading-snug">{ANNOTATION_STEPS[1].title}</h3>
              <p className="text-black/65 leading-relaxed text-[1.05rem]">{ANNOTATION_STEPS[1].body}</p>
            </SplotchCard>
          </div>
        </div>

        {/* Right column: sticky code */}
        <div className="flex-1 relative">
          <div className="sticky top-[15vh]">
            <CodeBlock step={step} />
          </div>
        </div>
      </div>

      {/* Mobile */}
      <div className="md:hidden max-w-lg mx-auto relative">
        <div className="pt-24 pb-12">
          <Hero />
        </div>
        <div className="mb-8">
          <CodeBlock step={0} />
        </div>
        <div className="space-y-12 pb-12">
          {ANNOTATION_STEPS.map((s, idx) => (
            <div key={idx}>
              <div className="mb-5">
                <SplotchCard idx={idx} active={true}>
                  <span className="font-heading text-black/20 text-3xl font-bold mb-3 block">{s.num}</span>
                  <h3 className="font-heading text-2xl font-bold text-black mb-3 leading-snug">{s.title}</h3>
                  <p className="text-black/65 text-base leading-relaxed">{s.body}</p>
                </SplotchCard>
              </div>
              <CodeBlock step={(idx + 1) as Step} />
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}
