import { useEffect, useState } from 'react'
import type { FormEvent, ReactNode } from 'react'
import { NavLink, useNavigate } from 'react-router-dom'
import { api } from './api'
import { ComplianceAuthNotice, ComplianceSiteFooter } from './compliance'

export function useAuth() {
  const [user, setUser] = useState<any | null | undefined>(undefined)
  useEffect(() => {
    api.me().then(setUser).catch(() => setUser(null))
  }, [])
  return { user, setUser, refresh: () => api.me().then(setUser) }
}

export function AuthPage({ onLogin }: { onLogin: (u: any) => void }) {
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const navigate = useNavigate()

  async function submit(e: FormEvent) {
    e.preventDefault()
    setError('')
    try {
      const u =
        mode === 'login'
          ? await api.login(username, password)
          : await api.register(username, password)
      onLogin(u)
      navigate('/')
    } catch (err: any) {
      setError(err.message)
    }
  }

  return (
    <div className="auth-page">
      <form className="card" onSubmit={submit}>
        <h1>用户中心</h1>
        <p className="muted">danmo-router User Portal</p>
        {error && <div className="error">{error}</div>}
        <label>
          用户名
          <input value={username} onChange={(e) => setUsername(e.target.value)} required />
        </label>
        <label>
          密码
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            minLength={6}
          />
        </label>
        <button type="submit">{mode === 'login' ? '登录' : '注册'}</button>
        <button
          type="button"
          className="ghost"
          onClick={() => setMode(mode === 'login' ? 'register' : 'login')}
        >
          {mode === 'login' ? '没有账号？注册' : '已有账号？登录'}
        </button>
        <ComplianceAuthNotice />
      </form>
    </div>
  )
}

export function Layout({
  user,
  onLogout,
  children,
}: {
  user: any
  onLogout: () => void
  children: ReactNode
}) {
  return (
    <div className="layout">
      <header>
        <div className="brand">Danmo User</div>
        <nav>
          <NavLink to="/">概览</NavLink>
          <NavLink to="/tokens">令牌</NavLink>
          <NavLink to="/models">模型</NavLink>
          <NavLink to="/logs">日志</NavLink>
        </nav>
        <div className="header-right">
          <span>{user.username}</span>
          <button className="ghost" onClick={onLogout}>
            退出
          </button>
        </div>
      </header>
      <div className="layout-body">
        <main>{children}</main>
        <ComplianceSiteFooter />
      </div>
    </div>
  )
}

export const PAGE_SIZE = 50
