import { CodeStorytelling } from './components/CodeStorytelling'
import { PlaygroundSection } from './components/PlaygroundSection'
import { SpeedSection } from './components/SpeedSection'
import { CTASection } from './components/CTASection'
import { ApiDocs } from './components/ApiDocs'
import { Footer } from './components/Footer'

function App() {
  return (
    <div className="relative min-h-screen bg-mustard">
      <main className="relative z-10">
        <CodeStorytelling />
        <PlaygroundSection />
        <SpeedSection />
        <CTASection />
        <ApiDocs />
        <Footer />
      </main>
    </div>
  )
}

export default App
