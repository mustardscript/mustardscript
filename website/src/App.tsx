import { Routes, Route, Navigate } from 'react-router-dom'
import { CodeStorytelling } from './components/CodeStorytelling'
import { SpeedSection } from './components/SpeedSection'
import { CTASection } from './components/CTASection'
import { ApiDocs } from './components/ApiDocs'
import { Footer } from './components/Footer'
import { LandingNavbar } from './components/LandingNavbar'
import { DocsLayout } from './components/docs/DocsLayout'
import { DocsIndex } from './components/docs/DocsIndex'
import { DocPage } from './components/docs/DocPage'

function LandingPage() {
  return (
    <div className="relative min-h-screen bg-mustard">
      <LandingNavbar />
      <main className="relative z-10">
        <CodeStorytelling />
        <SpeedSection />
        <CTASection />
        <ApiDocs />
        <Footer />
      </main>
    </div>
  )
}

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<LandingPage />} />
      <Route path="/docs" element={<DocsLayout />}>
        <Route index element={<DocsIndex />} />
        <Route path=":slug" element={<DocPage />} />
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  )
}
