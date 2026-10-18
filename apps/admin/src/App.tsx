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
  return { user, setUser }
}

function LoginPage({ onLogin }: { onLogin: (u: any) => void }) {
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
      <aside>
        <div className="brand">Danmo Admin</div>
        <nav>
          <NavLink to="/">概览</NavLink>
          <NavLink to="/channels">渠道</NavLink>
          <NavLink to="/users">用户</NavLink>
          <NavLink to="/prices">倍率</NavLink>
          <NavLink to="/logs">日志</NavLink>
        </nav>
        <div className="aside-footer">
          <div>{user.username}</div>
          <button className="linkish" onClick={onLogout}>
            退出
          </button>
        </div>
      </aside>
      <main>{children}</main>
    </div>
  )
}

function OverviewPage() {
  const [stats, setStats] = useState<any>(null)
  useEffect(() => {
    api.overview().then(setStats).catch(console.error)
  }, [])
  if (!stats) return <p>加载中...</p>
  const rate =
    stats.total_requests > 0
      ? ((stats.success_requests / stats.total_requests) * 100).toFixed(1)
      : '0.0'
  return (
    <div>
      <h1>概览</h1>
      <div className="grid">
        <div className="stat">
          <div className="label">总请求</div>
          <div className="value">{stats.total_requests}</div>
        </div>
        <div className="stat">
          <div className="label">成功率</div>
          <div className="value">{rate}%</div>
        </div>
        <div className="stat">
          <div className="label">消耗额度</div>
          <div className="value">{stats.total_quota}</div>
        </div>
        <div className="stat">
          <div className="label">用户 / 渠道</div>
          <div className="value">
            {stats.total_users} / {stats.total_channels}
          </div>
        </div>
      </div>
    </div>
  )
}

function ChannelsPage() {
  const [channels, setChannels] = useState<any[]>([])
  const [form, setForm] = useState({
    name: '',
    type: 'openai_compat',
    key: '',
    base_url: 'https://api.openai.com',
    models: 'gpt-4o-mini',
    group_name: 'default',
    priority: 0,
    weight: 1,
    status: 'enabled',
  })
  const [error, setError] = useState('')

  async function refresh() {
    setChannels(await api.channels())
  }
  useEffect(() => {
    refresh().catch(console.error)
  }, [])

  async function create(e: FormEvent) {
    e.preventDefault()
    setError('')
    try {
      await api.createChannel(form)
      setForm({ ...form, name: '', key: '' })
      await refresh()
    } catch (err: any) {
      setError(err.message)
    }
  }

  return (
    <div>
      <h1>渠道</h1>
      {error && <div className="error">{error}</div>}
      <form className="card form-grid" onSubmit={create}>
        <input
          placeholder="名称"
          value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })}
          required
        />
        <input
          placeholder="Base URL"
          value={form.base_url}
          onChange={(e) => setForm({ ...form, base_url: e.target.value })}
          required
        />
        <input
          placeholder="上游 API Key"
          value={form.key}
          onChange={(e) => setForm({ ...form, key: e.target.value })}
          required
        />
        <input
          placeholder="模型列表，逗号分隔"
          value={form.models}
          onChange={(e) => setForm({ ...form, models: e.target.value })}
          required
        />
        <input
          placeholder="分组"
          value={form.group_name}
          onChange={(e) => setForm({ ...form, group_name: e.target.value })}
        />
        <input
          type="number"
          placeholder="优先级"
          value={form.priority}
          onChange={(e) => setForm({ ...form, priority: Number(e.target.value) })}
        />
        <button type="submit">新增渠道</button>
      </form>
      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>名称</th>
            <th>Base URL</th>
            <th>模型</th>
            <th>分组</th>
            <th>优先级</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {channels.map((c) => (
            <tr key={c.id}>
              <td>{c.id}</td>
              <td>{c.name}</td>
              <td>{c.base_url}</td>
              <td>{c.models}</td>
              <td>{c.group_name}</td>
              <td>{c.priority}</td>
              <td>{c.status}</td>
              <td className="actions">
                <button
                  onClick={async () => {
                    await api.setChannelStatus(
                      c.id,
                      c.status === 'enabled' ? 'disabled' : 'enabled',
                    )
                    await refresh()
                  }}
                >
                  {c.status === 'enabled' ? '停用' : '启用'}
                </button>
                <button
                  className="danger"
                  onClick={async () => {
                    await api.deleteChannel(c.id)
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

function UsersPage() {
  const [users, setUsers] = useState<any[]>([])
  const [form, setForm] = useState({
    username: '',
    password: '',
    quota: 500000,
    group_name: 'default',
  })

  async function refresh() {
    setUsers(await api.users())
  }
  useEffect(() => {
    refresh().catch(console.error)
  }, [])

  return (
    <div>
      <h1>用户</h1>
      <form
        className="card form-grid"
        onSubmit={async (e) => {
          e.preventDefault()
          await api.createUser(form)
          setForm({ ...form, username: '', password: '' })
          await refresh()
        }}
      >
        <input
          placeholder="用户名"
          value={form.username}
          onChange={(e) => setForm({ ...form, username: e.target.value })}
          required
        />
        <input
          placeholder="密码"
          type="password"
          value={form.password}
          onChange={(e) => setForm({ ...form, password: e.target.value })}
          required
        />
        <input
          type="number"
          placeholder="额度"
          value={form.quota}
          onChange={(e) => setForm({ ...form, quota: Number(e.target.value) })}
        />
        <input
          placeholder="分组"
          value={form.group_name}
          onChange={(e) => setForm({ ...form, group_name: e.target.value })}
        />
        <button type="submit">创建用户</button>
      </form>
      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>用户名</th>
            <th>角色</th>
            <th>额度</th>
            <th>已用</th>
            <th>分组</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {users.map((u) => (
            <tr key={u.id}>
              <td>{u.id}</td>
              <td>{u.username}</td>
              <td>{u.role}</td>
              <td>{u.quota}</td>
              <td>{u.used_quota}</td>
              <td>{u.group_name}</td>
              <td>{u.status}</td>
              <td className="actions">
                <button
                  onClick={async () => {
                    await api.adjustQuota(u.id, 100000)
                    await refresh()
                  }}
                >
                  +10万额度
                </button>
                <button
                  onClick={async () => {
                    await api.updateUser(u.id, {
                      status: u.status === 'active' ? 'disabled' : 'active',
                    })
                    await refresh()
                  }}
                >
                  {u.status === 'active' ? '禁用' : '启用'}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function PricesPage() {
  const [prices, setPrices] = useState<any[]>([])
  const [form, setForm] = useState({
    model: '',
    model_ratio: 1,
    completion_ratio: 1,
  })
  async function refresh() {
    setPrices(await api.modelPrices())
  }
  useEffect(() => {
    refresh().catch(console.error)
  }, [])
  return (
    <div>
      <h1>模型倍率</h1>
      <form
        className="card form-grid"
        onSubmit={async (e) => {
          e.preventDefault()
          await api.upsertModelPrice(form)
          setForm({ model: '', model_ratio: 1, completion_ratio: 1 })
          await refresh()
        }}
      >
        <input
          placeholder="模型名"
          value={form.model}
          onChange={(e) => setForm({ ...form, model: e.target.value })}
          required
        />
        <input
          type="number"
          step="0.01"
          placeholder="model_ratio"
          value={form.model_ratio}
          onChange={(e) => setForm({ ...form, model_ratio: Number(e.target.value) })}
        />
        <input
          type="number"
          step="0.01"
          placeholder="completion_ratio"
          value={form.completion_ratio}
          onChange={(e) =>
            setForm({ ...form, completion_ratio: Number(e.target.value) })
          }
        />
        <button type="submit">保存</button>
      </form>
      <table>
        <thead>
          <tr>
            <th>模型</th>
            <th>Model Ratio</th>
            <th>Completion Ratio</th>
          </tr>
        </thead>
        <tbody>
          {prices.map((p) => (
            <tr key={p.id}>
              <td>{p.model}</td>
              <td>{p.model_ratio}</td>
              <td>{p.completion_ratio}</td>
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
      <h1>调用日志</h1>
      <table>
        <thead>
          <tr>
            <th>时间</th>
            <th>用户</th>
            <th>模型</th>
            <th>Prompt</th>
            <th>Completion</th>
            <th>额度</th>
            <th>耗时</th>
            <th>状态</th>
          </tr>
        </thead>
        <tbody>
          {logs.map((l) => (
            <tr key={l.id}>
              <td>{l.created_at}</td>
              <td>{l.user_id}</td>
              <td>{l.model}</td>
              <td>{l.prompt_tokens}</td>
              <td>{l.completion_tokens}</td>
              <td>{l.quota}</td>
              <td>{l.duration_ms}ms</td>
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
        <Route path="/logs" element={<LogsPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Layout>
  )
}
