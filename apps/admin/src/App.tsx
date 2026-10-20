import { Navigate, Route, Routes } from 'react-router-dom'
import { api } from './api'
import { ChannelsPage } from './pages/ChannelsPage'
import { LogsPage } from './pages/LogsPage'
import { OverviewPage } from './pages/OverviewPage'
import { PricesPage } from './pages/PricesPage'
import { RedemptionsPage } from './pages/RedemptionsPage'
import { UsersPage } from './pages/UsersPage'
import { Layout, LoginPage, useAuth } from './shared'
import './App.css'

export default function App() {
  const { user, setUser } = useAuth()
  if (user === undefined) return <div className="auth-page">加载中...</div>

  if (!user) {
    return <LoginPage onLogin={setUser} />
  }

  return (
    <Layout
      user={user}
      onLogout={async () => {
        await api.logout()
        setUser(null)
      }}
    >
      <Routes>
        <Route path="/" element={<OverviewPage />} />
        <Route path="/channels" element={<ChannelsPage />} />
        <Route path="/users" element={<UsersPage />} />
        <Route path="/prices" element={<PricesPage />} />
        <Route path="/redemptions" element={<RedemptionsPage />} />
        <Route path="/logs" element={<LogsPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Layout>
  )
}
