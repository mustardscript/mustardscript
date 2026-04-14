import { useState } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { highlightCode } from './highlight'

interface Tab {
  id: string
  label: string
  description: string
  code: string
  filename: string
}

const tabs: Tab[] = [
  {
    id: 'simple', label: 'Quick Start',
    description: 'Two lines to run sandboxed JavaScript.',
    filename: 'hello.ts',
    code: `import { Mustard } from 'mustardscript';

const program = new Mustard('const x = 2 + 2; x;');
const result = await program.run();

console.log(result); // 4`,
  },
  {
    id: 'capabilities', label: 'Tool Calling',
    description: 'Expose host functions that guest code calls by name.',
    filename: 'tool-calling.ts',
    code: `import { Mustard } from 'mustardscript';

const program = new Mustard(\`
  const user = fetch_user(userId);
  const posts = fetch_posts(user.id);
  ({ name: user.name, postCount: posts.length });
\`);

const result = await program.run({
  inputs: { userId: 123 },
  capabilities: {
    fetch_user(id) {
      return db.users.findById(id);
    },
    fetch_posts(userId) {
      return db.posts.where({ userId });
    },
  },
});`,
  },
  {
    id: 'resumable', label: 'Suspend & Resume',
    description: 'Pause at capability boundaries. Persist. Resume from any process.',
    filename: 'suspend-resume.ts',
    code: `import { Mustard, Progress } from 'mustardscript';

const program = new Mustard(\`
  const profile = fetch_profile(userId);
  profile.name;
\`);

const step = program.start({
  inputs: { userId: 7 },
  capabilities: { fetch_profile() {} },
});

if (step instanceof Progress) {
  const snapshot = step.dump();
  await redis.set(\`job:\${id}\`, JSON.stringify(snapshot));

  // Resume later — even in a different process
  const restored = Progress.load(snapshot, {
    capabilities: { fetch_profile() {} },
    limits: {},
  });
  const result = restored.resume({ id: 7, name: 'Ada' });
}`,
  },
  {
    id: 'limits', label: 'Resource Limits',
    description: 'Hard caps on CPU, memory, depth, and wall-clock time.',
    filename: 'with-limits.ts',
    code: `import { Mustard } from 'mustardscript';

const program = new Mustard(untrustedCode);

const result = await program.run({
  capabilities: { /* ... */ },
  limits: {
    instructionBudget: 100_000,
    heapLimitBytes: 4 * 1024 * 1024,
    allocationBudget: 10_000,
    callDepthLimit: 64,
    maxOutstandingHostCalls: 16,
  },
  signal: AbortSignal.timeout(5000),
});
// Throws MustardLimitError if any limit is exceeded`,
  },
  {
    id: 'executor', label: 'Batch Jobs',
    description: 'Built-in job queue with concurrent workers.',
    filename: 'batch-jobs.ts',
    code: `import {
  Mustard, MustardExecutor,
  InMemoryMustardExecutorStore,
} from 'mustardscript';

const executor = new MustardExecutor({
  program: new Mustard('seed * 2;', { inputs: ['seed'] }),
  capabilities: {},
  store: new InMemoryMustardExecutorStore(),
  limits: { instructionBudget: 100_000 },
});

const id1 = await executor.enqueue({ seed: 5 });
const id2 = await executor.enqueue({ seed: 42 });

await executor.runWorker({
  maxConcurrentJobs: 4,
  drain: true,
});

const job = await executor.get(id1);
console.log(job.state);  // 'completed'
console.log(job.result); // 10`,
  },
]

// highlightCode imported from ./highlight

export function ApiDocs() {
  const [activeTab, setActiveTab] = useState('simple')
  const activeContent = tabs.find((t) => t.id === activeTab)!

  return (
    <section className="py-24 px-6 scroll-mt-20" id="examples">
      <div className="max-w-3xl mx-auto">
        <div className="text-center mb-14">
          <h2 className="font-heading text-3xl sm:text-4xl md:text-5xl font-bold mb-4 tracking-tight text-black">
            Get started in minutes
          </h2>
          <p className="text-black/50 text-lg max-w-md mx-auto">
            From a two-line eval to production job queues. Pick your use case.
          </p>
        </div>

        {/* Tabs */}
        <div className="flex flex-wrap gap-2 mb-8 justify-center">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`px-5 py-2.5 rounded-lg text-sm font-semibold transition-all duration-200 cursor-pointer ${
                activeTab === tab.id
                  ? 'bg-black text-white shadow-md'
                  : 'bg-black/6 text-black/60 hover:bg-black/12 hover:text-black'
              }`}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {/* Content */}
        <AnimatePresence mode="wait">
          <motion.div
            key={activeTab}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            transition={{ duration: 0.2 }}
          >
            <p className="text-black/50 text-base mb-5 text-center">
              {activeContent.description}
            </p>
            <div className="code-block shadow-xl shadow-black/20">
              <div className="flex items-center gap-2 px-5 py-2.5 border-b border-amber-900/30 bg-black/20">
                <div className="w-2.5 h-2.5 rounded-full bg-[#ff5f57]" />
                <div className="w-2.5 h-2.5 rounded-full bg-[#febc2e]" />
                <div className="w-2.5 h-2.5 rounded-full bg-[#28c840]" />
                <span className="ml-3 text-xs font-mono text-amber-700/60">{activeContent.filename}</span>
              </div>
              <pre className="p-6 text-sm leading-7 font-mono overflow-x-auto text-[#D4C8A8]">
                <code dangerouslySetInnerHTML={{ __html: highlightCode(activeContent.code) }} />
              </pre>
            </div>
          </motion.div>
        </AnimatePresence>
      </div>
    </section>
  )
}
