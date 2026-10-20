import { Navigate, Route, Routes } from 'react-router-dom'
import { api } from './api'
import { HomePage } from './pages/HomePage'
import { LogsPage } from './pages/LogsPage'
import { ModelsPage } from './pages/ModelsPage'
import { TokensPage } from './pages/TokensPage'
import { AuthPage, Layout, useAuth } from './shared'
import './App.css'

export default function App() {
  const { user, setUser } = useAuth()
  if (user === undefined) return <div className="auth-page">加载中...</div>
  if (!user) return <AuthPage onLogin={setUser} />

  return (
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
  )
}
