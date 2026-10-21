import { Navigate, Route, Routes } from 'react-router-dom'
import { api } from './api'
import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import { ChannelFormPage } from './pages/ChannelFormPage'
import { ChannelsPage } from './pages/ChannelsPage'
import { LogsPage } from './pages/LogsPage'
import { OverviewPage } from './pages/OverviewPage'
import { PriceFormPage, PricesPage } from './pages/PricesPage'
import { RedemptionsPage } from './pages/RedemptionsPage'
import { UsersPage } from './pages/UsersPage'
import { Layout, LoginPage, useAuth } from './shared'

export default function App() {
  const { user, setUser } = useAuth()

  return (
    <TooltipProvider>
      <Toaster position="top-right" />
      {user === undefined ? (
        <div className="grid min-h-svh place-items-center text-sm text-muted-foreground">加载中...</div>
      ) : !user ? (
        <LoginPage onLogin={setUser} />
      ) : (
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
            <Route path="/channels/new" element={<ChannelFormPage />} />
            <Route path="/channels/:id/edit" element={<ChannelFormPage />} />
            <Route path="/users" element={<UsersPage />} />
            <Route path="/prices" element={<PricesPage />} />
            <Route path="/prices/new" element={<PriceFormPage />} />
            <Route path="/prices/:model/edit" element={<PriceFormPage />} />
            <Route path="/redemptions" element={<RedemptionsPage />} />
            <Route path="/logs" element={<LogsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </Layout>
      )}
    </TooltipProvider>
  )
}
