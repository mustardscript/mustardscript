import { useState, useEffect } from 'react'
import { Outlet, useLocation } from 'react-router-dom'
import { DocsNavbar } from './DocsNavbar'
import { DocsSidebar } from './DocsSidebar'

export function DocsLayout() {
  const [sidebarOpen, setSidebarOpen] = useState(false)
  const location = useLocation()

  // Close sidebar on navigation
  useEffect(() => {
    setSidebarOpen(false)
  }, [location.pathname])

  return (
    <div className="min-h-screen bg-[#FFFDF7] dark:bg-[#0A0A0B]">
      <DocsNavbar onToggleSidebar={() => setSidebarOpen(!sidebarOpen)} />

      <div className="max-w-[1400px] mx-auto flex">
        {/* Sidebar — desktop */}
        <aside className="hidden lg:block w-64 shrink-0 sticky top-14 max-h-[calc(100vh-3.5rem)] overflow-y-auto border-r border-black/5 dark:border-white/8 bg-[#FFF8E1]/50 dark:bg-[#111113]/50">
          <DocsSidebar />
        </aside>

        {/* Sidebar — mobile overlay */}
        {sidebarOpen && (
          <>
            <div
              className="fixed inset-0 z-40 bg-black/20 dark:bg-black/60 lg:hidden"
              onClick={() => setSidebarOpen(false)}
            />
            <aside className="fixed inset-y-0 left-0 z-50 w-72 bg-[#FFF8E1] dark:bg-[#111113] shadow-xl overflow-y-auto lg:hidden">
              <div className="flex items-center justify-between px-4 h-14 border-b border-black/8 dark:border-white/8">
                <span className="font-heading font-bold text-sm dark:text-white">Docs</span>
                <button
                  onClick={() => setSidebarOpen(false)}
                  className="p-1.5 rounded-lg hover:bg-black/5 dark:hover:bg-white/10 dark:text-white transition-colors"
                  aria-label="Close sidebar"
                >
                  <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
                    <path d="M4 4l10 10M14 4L4 14" />
                  </svg>
                </button>
              </div>
              <DocsSidebar onNavigate={() => setSidebarOpen(false)} />
            </aside>
          </>
        )}

        {/* Main content */}
        <main className="flex-1 min-w-0 px-6 py-10 lg:px-10">
          <Outlet />
        </main>
      </div>
    </div>
  )
}
