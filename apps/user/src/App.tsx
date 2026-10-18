import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { Navigate, NavLink, Route, Routes, useNavigate } from 'react-router-dom'
import { api } from './api'
import './App.css'

function useAuth() {
  const [user, setUser] = useState<any | null | undefined>(undefined)
  useEffect(() => {
    api.me().then(setUser).catch(() => setUser(null))
  }, [])
  return { user, setUser, refresh: () => api.me().then(setUser) }
}

function AuthPage({ onLogin }: { onLogin: (u: any) => void }) {
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
      </form>
    </div>
  )
}

function Layout({
  user,
  onLogout,
  children,
}: {
  user: any
  onLogout: () => void
  children: React.ReactNode
}) {
  return (
    <div className="layout">
      <header>
        <div className="brand">Danmo User</div>
        <nav>
          <NavLink to="/">概览</NavLink>
          <NavLink to="/tokens">令牌</NavLink>
          <NavLink to="/logs">日志</NavLink>
        </nav>
        <div className="header-right">
          <span>{user.username}</span>
          <button className="ghost" onClick={onLogout}>
            退出
          </button>
        </div>
      </header>
      <main>{children}</main>
    </div>
  )
}

function HomePage({ user }: { user: any }) {
  return (
    <div>
      <h1>账户概览</h1>
      <div className="grid">
        <div className="stat">
          <div className="label">剩余额度</div>
          <div className="value">{user.remaining_quota ?? user.quota - user.used_quota}</div>
        </div>
        <div className="stat">
          <div className="label">总额度</div>
          <div className="value">{user.quota}</div>
        </div>
        <div className="stat">
          <div className="label">已用额度</div>
          <div className="value">{user.used_quota}</div>
        </div>
        <div className="stat">
          <div className="label">分组</div>
          <div className="value">{user.group_name}</div>
        </div>
      </div>
      <div className="card tip">
        <h3>调用方式</h3>
        <p>
          Base URL: <code>http://127.0.0.1:3000/v1</code>
        </p>
        <p>
          Header: <code>Authorization: Bearer sk-...</code>
        </p>
      </div>
    </div>
  )
}

function TokensPage() {
  const [tokens, setTokens] = useState<any[]>([])
  const [name, setName] = useState('default')
  const [createdKey, setCreatedKey] = useState('')
  const [error, setError] = useState('')

  async function refresh() {
    setTokens(await api.tokens())
  }
  useEffect(() => {
    refresh().catch(console.error)
  }, [])

  return (
    <div>
      <h1>API 令牌</h1>
      {error && <div className="error">{error}</div>}
      {createdKey && (
        <div className="card success">
          新令牌（仅显示一次）：<code>{createdKey}</code>
        </div>
      )}
      <form
        className="card form-row"
        onSubmit={async (e) => {
          e.preventDefault()
          setError('')
          try {
            const created = await api.createToken({
              name,
              unlimited_quota: true,
              models: '',
            })
            setCreatedKey(created.key)
            setName('default')
            await refresh()
          } catch (err: any) {
            setError(err.message)
          }
        }}
      >
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="令牌名称"
          required
        />
        <button type="submit">创建令牌</button>
      </form>
      <table>
        <thead>
          <tr>
            <th>名称</th>
            <th>前缀</th>
            <th>状态</th>
            <th>额度</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {tokens.map((t) => (
            <tr key={t.id}>
              <td>{t.name}</td>
              <td>{t.key_prefix}...</td>
              <td>{t.status}</td>
              <td>{t.unlimited_quota ? '无限' : t.remain_quota}</td>
              <td className="actions">
                <button
                  onClick={async () => {
                    await api.setTokenStatus(
                      t.id,
                      t.status === 'active' ? 'disabled' : 'active',
                    )
                    await refresh()
                  }}
                >
                  {t.status === 'active' ? '禁用' : '启用'}
                </button>
                <button
                  className="danger"
                  onClick={async () => {
                    await api.deleteToken(t.id)
                    await refresh()
                  }}
                >
                  删除
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function LogsPage() {
  const [logs, setLogs] = useState<any[]>([])
  useEffect(() => {
    api.logs().then(setLogs).catch(console.error)
  }, [])
  return (
    <div>
      <h1>我的调用日志</h1>
      <table>
        <thead>
          <tr>
            <th>时间</th>
            <th>模型</th>
            <th>Prompt</th>
            <th>Completion</th>
            <th>额度</th>
            <th>状态</th>
          </tr>
        </thead>
        <tbody>
          {logs.map((l) => (
            <tr key={l.id}>
              <td>{l.created_at}</td>
              <td>{l.model}</td>
              <td>{l.prompt_tokens}</td>
              <td>{l.completion_tokens}</td>
              <td>{l.quota}</td>
              <td>{l.status}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

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
        <Route path="/" element={<HomePage user={user} />} />
        <Route path="/tokens" element={<TokensPage />} />
        <Route path="/logs" element={<LogsPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Layout>
  )
}
