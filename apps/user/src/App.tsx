import { Navigate, Route, Routes } from 'react-router-dom'
import { api } from './api'
import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import { HomePage } from './pages/HomePage'
import { LogsPage } from './pages/LogsPage'
import { ModelsPage } from './pages/ModelsPage'
import { TokensPage } from './pages/TokensPage'
import { AuthPage, Layout, useAuth } from './shared'

export default function App() {
  const { user, setUser } = useAuth()

  return (
    <TooltipProvider>
      <Toaster position="top-right" />
      {user === undefined ? (
        <div className="grid min-h-svh place-items-center text-sm text-muted-foreground">加载中...</div>
      ) : !user ? (
        <AuthPage onLogin={setUser} />
      ) : (
        <Layout
          user={user}
          onLogout={async () => {
            await api.logout()
            setUser(null)
          }}
        >
          <Routes>
            <Route path="/" element={<HomePage user={user} onUser={setUser} />} />
            <Route path="/tokens" element={<TokensPage />} />
            <Route path="/models" element={<ModelsPage />} />
            <Route path="/logs" element={<LogsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </Layout>
      )}
    </TooltipProvider>
  )
}
