import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { NavLink, useNavigate } from 'react-router-dom'
import type { FormEvent } from 'react'
import { api } from './api'
import type { Dialect, Meta } from './api'
import { ComplianceAuthNotice, ComplianceFooterLink } from './compliance'

export function useAuth() {
  const [user, setUser] = useState<any | null | undefined>(undefined)
  useEffect(() => {
    api.me().then(setUser).catch(() => setUser(null))
  }, [])
  return { user, setUser }
}

export function LoginPage({ onLogin }: { onLogin: (u: any) => void }) {
  const [username, setUsername] = useState('admin')
  const [password, setPassword] = useState('admin123')
  const [error, setError] = useState('')
  const navigate = useNavigate()

  async function submit(e: FormEvent) {
    e.preventDefault()
    setError('')
    try {
      const u = await api.login(username, password)
      onLogin(u)
      navigate('/')
    } catch (err: any) {
      setError(err.message)
    }
  }

  return (
    <div className="auth-page">
      <form className="card" onSubmit={submit}>
        <h1>管理后台</h1>
        <p className="muted">danmo-router Admin Portal</p>
        {error && <div className="error">{error}</div>}
        <label>
          用户名
          <input value={username} onChange={(e) => setUsername(e.target.value)} />
        </label>
        <label>
          密码
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        <button type="submit">登录</button>
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
      <aside>
        <div className="brand">Danmo Admin</div>
        <nav>
          <NavLink to="/">概览</NavLink>
          <NavLink to="/channels">渠道</NavLink>
          <NavLink to="/users">用户</NavLink>
          <NavLink to="/prices">价格</NavLink>
          <NavLink to="/redemptions">兑换码</NavLink>
          <NavLink to="/logs">日志</NavLink>
        </nav>
        <div className="aside-bottom">
          <ComplianceFooterLink />
          <div className="aside-footer">
            <div>{user.username}</div>
            <button className="linkish" onClick={onLogout}>
              退出
            </button>
          </div>
        </div>
      </aside>
      <main>{children}</main>
    </div>
  )
}

export function useMeta() {
  const [meta, setMeta] = useState<Meta | null>(null)
  useEffect(() => {
    api.meta().then(setMeta).catch(console.error)
  }, [])
  return meta
}

export function dialectsOf(meta: Meta | null, protocol: string): Dialect[] {
  const all = meta?.dialects.filter((d) => d.protocol === protocol) ?? []
  return all.length > 0 ? all : [{ protocol, name: 'standard', label: '', opaque: false, usage: [] }]
}

export function DialectSelect({
  meta,
  protocol,
  value,
  onChange,
}: {
  meta: Meta | null
  protocol: string
  value: string
  onChange: (v: string) => void
}) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)}>
      {dialectsOf(meta, protocol).map((d) => (
        <option key={d.name} value={d.name} title={d.label}>
          {d.name}
        </option>
      ))}
    </select>
  )
}

export const PAGE_SIZE = 50
