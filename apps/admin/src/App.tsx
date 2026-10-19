import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { Navigate, NavLink, Route, Routes, useNavigate } from 'react-router-dom'
import { api } from './api'
import type { Dialect, Meta } from './api'
import { CHANNEL_PRESETS, MODE_LABELS, PROTOCOL_LABELS, dialectForModel, suggestNativeDialects } from './presets'
import type { DialectRule } from './presets'
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
          <NavLink to="/prices">价格</NavLink>
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

type ModelRow = { model: string; upstream_model: string; upstream_dialect: string }

const EMPTY_CHANNEL = {
  name: '',
  type: 'openai',
  vendor: 'openai',
  key: '',
  base_url: 'https://api.openai.com/v1',
  group_name: 'default',
  priority: 0,
  weight: 1,
  status: 'enabled',
  relay_mode: 'passthrough',
  upstream_protocol: 'openai_chat',
  auth_style: 'bearer',
  inject_stream_usage: true,
  default_dialect: 'standard',
}

function useMeta() {
  const [meta, setMeta] = useState<Meta | null>(null)
  useEffect(() => {
    api.meta().then(setMeta).catch(console.error)
  }, [])
  return meta
}

function dialectsOf(meta: Meta | null, protocol: string): Dialect[] {
  const all = meta?.dialects.filter((d) => d.protocol === protocol) ?? []
  return all.length > 0 ? all : [{ protocol, name: 'standard', label: '', opaque: false, usage: [] }]
}

function DialectSelect({
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

function ChannelsPage() {
  const meta = useMeta()
  const [channels, setChannels] = useState<any[]>([])
  const [form, setForm] = useState({ ...EMPTY_CHANNEL })
  const [rows, setRows] = useState<ModelRow[]>([])
  const [rules, setRules] = useState<DialectRule[]>([])
  const [newModels, setNewModels] = useState('gpt-4o-mini')
  const [editing, setEditing] = useState<number | null>(null)
  const [error, setError] = useState('')

  async function refresh() {
    setChannels(await api.channels())
  }
  useEffect(() => {
    refresh().catch(console.error)
  }, [])

  function rowsFor(names: string, defaultDialect: string, prefixRules: DialectRule[]): ModelRow[] {
    return names
      .split(',')
      .map((m) => m.trim())
      .filter(Boolean)
      .map((model) => ({
        model,
        upstream_model: '',
        upstream_dialect: dialectForModel(model, defaultDialect, prefixRules),
      }))
  }

  function applyPreset(id: string) {
    const p = CHANNEL_PRESETS.find((x) => x.id === id)
    if (!p) return
    setForm({
      ...form,
      name: form.name || p.label,
      type: p.type,
      vendor: p.vendor,
      base_url: p.base_url,
      upstream_protocol: p.upstream_protocol,
      auth_style: p.auth_style,
      default_dialect: p.default_dialect,
    })
    setRules(p.rules ?? [])
    setRows(rowsFor(p.models, p.default_dialect, p.rules ?? []))
    setNewModels('')
  }

  function addModels() {
    const existing = new Set(rows.map((r) => r.model))
    const added = rowsFor(newModels, form.default_dialect, rules).filter((r) => !existing.has(r.model))
    setRows([...rows, ...added])
    setNewModels('')
  }

  function setProtocol(protocol: string) {
    const names = new Set(dialectsOf(meta, protocol).map((d) => d.name))
    const fallback = names.has(form.default_dialect) ? form.default_dialect : 'standard'
    setForm({
      ...form,
      upstream_protocol: protocol,
      default_dialect: fallback,
      relay_mode: protocol === 'embeddings' ? 'passthrough' : form.relay_mode,
    })
    setRows(rows.map((r) => (names.has(r.upstream_dialect) ? r : { ...r, upstream_dialect: fallback })))
  }

  function edit(c: any) {
    setEditing(c.id)
    setForm({ ...EMPTY_CHANNEL, ...c, key: '' })
    setRows(
      (c.models ?? []).map((r: any) => ({
        model: r.model,
        upstream_model: r.upstream_model,
        upstream_dialect: r.upstream_dialect,
      })),
    )
    setRules([])
    setNewModels('')
  }

  function reset() {
    setEditing(null)
    setForm({ ...EMPTY_CHANNEL })
    setRows([])
    setRules([])
    setNewModels('gpt-4o-mini')
  }

  async function submit(e: FormEvent) {
    e.preventDefault()
    setError('')
    const pending = rowsFor(newModels, form.default_dialect, rules).filter(
      (r) => !rows.some((x) => x.model === r.model),
    )
    const body = { ...form, models: [...rows, ...pending] }
    try {
      if (editing !== null) {
        await api.updateChannel(editing, body)
        reset()
      } else {
        await api.createChannel(body)
        setForm({ ...form, name: '', key: '' })
        setRows([])
        setNewModels('')
      }
      await refresh()
    } catch (err: any) {
      setError(err.message)
    }
  }

  const groups = [...new Set(CHANNEL_PRESETS.map((p) => p.group))]
  const canConvert = form.upstream_protocol !== 'embeddings'

  return (
    <div>
      <h1>渠道</h1>
      {error && <div className="error">{error}</div>}
      <form className="card form-grid" onSubmit={submit}>
        <label className="wide">
          预设（只填充表单）
          <select value="" onChange={(e) => applyPreset(e.target.value)}>
            <option value="" disabled>
              选择原厂或三方渠道…
            </option>
            {groups.map((g) => (
              <optgroup key={g} label={g}>
                {CHANNEL_PRESETS.filter((p) => p.group === g).map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </optgroup>
            ))}
          </select>
        </label>
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
          placeholder={editing !== null ? '上游 API Key（留空不改）' : '上游 API Key'}
          value={form.key}
          onChange={(e) => setForm({ ...form, key: e.target.value })}
          required={editing === null}
        />
        <label>
          上游协议
          <select value={form.upstream_protocol} onChange={(e) => setProtocol(e.target.value)}>
            {(meta?.protocols ?? Object.keys(PROTOCOL_LABELS)).map((p) => (
              <option key={p} value={p}>
                {PROTOCOL_LABELS[p] ?? p}
              </option>
            ))}
          </select>
        </label>
        <label>
          转发模式
          <select
            value={form.relay_mode}
            onChange={(e) => setForm({ ...form, relay_mode: e.target.value })}
          >
            {(meta?.relay_modes ?? ['passthrough', 'convert'])
              .filter((m) => canConvert || m === 'passthrough')
              .map((m) => (
                <option key={m} value={m}>
                  {MODE_LABELS[m] ?? m}
                </option>
              ))}
          </select>
        </label>
        <label>
          鉴权方式
          <select
            value={form.auth_style}
            onChange={(e) => setForm({ ...form, auth_style: e.target.value })}
          >
            {(meta?.auth_styles ?? ['bearer', 'x_api_key', 'x_goog_api_key']).map((a) => (
              <option key={a} value={a}>
                {a}
              </option>
            ))}
          </select>
        </label>
        <label>
          新模型默认方言
          <DialectSelect
            meta={meta}
            protocol={form.upstream_protocol}
            value={form.default_dialect}
            onChange={(v) => setForm({ ...form, default_dialect: v })}
          />
        </label>
        <input
          placeholder="原厂标识（vendor，聚合渠道留空）"
          value={form.vendor}
          onChange={(e) => setForm({ ...form, vendor: e.target.value })}
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
        <label className="checkbox">
          <input
            type="checkbox"
            checked={form.inject_stream_usage}
            onChange={(e) => setForm({ ...form, inject_stream_usage: e.target.checked })}
          />
          流式请求补 include_usage（计费需要）
        </label>
        <div className="wide">
          <table className="rows">
            <thead>
              <tr>
                <th>模型名（客户端）</th>
                <th>上游模型名（空 = 同名）</th>
                <th>上游方言</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r, i) => (
                <tr key={r.model}>
                  <td>{r.model}</td>
                  <td>
                    <input
                      value={r.upstream_model}
                      placeholder={r.model}
                      onChange={(e) =>
                        setRows(rows.map((x, j) => (j === i ? { ...x, upstream_model: e.target.value } : x)))
                      }
                    />
                  </td>
                  <td>
                    <DialectSelect
                      meta={meta}
                      protocol={form.upstream_protocol}
                      value={r.upstream_dialect}
                      onChange={(v) =>
                        setRows(rows.map((x, j) => (j === i ? { ...x, upstream_dialect: v } : x)))
                      }
                    />
                  </td>
                  <td>
                    <button type="button" className="danger" onClick={() => setRows(rows.filter((_, j) => j !== i))}>
                      移除
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="row-add">
            <input
              placeholder="添加模型，逗号分隔（按默认方言和预设前缀规则填方言）"
              value={newModels}
              onChange={(e) => setNewModels(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault()
                  addModels()
                }
              }}
            />
            <button type="button" onClick={addModels}>
              添加
            </button>
          </div>
        </div>
        <p className="hint wide">
          方言是上游对该模型实际遵循的厂商规范。客户端协议与该模型的原厂方言都和这一行一致时原样透传，否则经中间表示转换；opaque（聚合渠道）只透传同协议请求。
          转发模式只决定能否服务其他协议。
        </p>
        <div className="actions wide">
          <button type="submit">{editing !== null ? '保存渠道' : '新增渠道'}</button>
          {editing !== null && (
            <button type="button" onClick={reset}>
              取消编辑
            </button>
          )}
        </div>
      </form>
      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>名称</th>
            <th>上游协议</th>
            <th>模式</th>
            <th>Base URL</th>
            <th>模型 · 方言</th>
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
              <td>{PROTOCOL_LABELS[c.upstream_protocol] ?? c.upstream_protocol}</td>
              <td>{c.relay_mode}</td>
              <td>{c.base_url}</td>
              <td className="mono">
                {(c.models ?? []).map((r: any) => (
                  <div key={r.id}>
                    {r.model}
                    {r.upstream_model ? ` → ${r.upstream_model}` : ''} · {r.upstream_dialect}
                  </div>
                ))}
              </td>
              <td>{c.group_name}</td>
              <td>{c.priority}</td>
              <td>{c.status}</td>
              <td className="actions">
                <button onClick={() => edit(c)}>编辑</button>
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

const EMPTY_PRICE = {
  model: '',
  model_ratio: 1,
  completion_ratio: 1,
  cache_read_ratio: 1,
  cache_write_ratio: 1,
  search_price: 0,
  tiers: '',
  periods: '',
}

/** Protocols where a model can have a non-standard native dialect. */
const NATIVE_PROTOCOLS = ['openai_chat', 'openai_responses', 'anthropic_messages']

function parseNative(raw: string): Record<string, string> {
  try {
    const v = JSON.parse(raw || '{}')
    return v && typeof v === 'object' && !Array.isArray(v) ? v : {}
  } catch {
    return {}
  }
}

function PricesPage() {
  const meta = useMeta()
  const [prices, setPrices] = useState<any[]>([])
  const [channels, setChannels] = useState<any[]>([])
  const [form, setForm] = useState({ ...EMPTY_PRICE })
  const [native, setNative] = useState<Record<string, string>>({})
  const [error, setError] = useState('')
  async function refresh() {
    setPrices(await api.modelPrices())
  }
  useEffect(() => {
    refresh().catch(console.error)
    api.channels().then(setChannels).catch(console.error)
  }, [])

  const suggestion = form.model ? suggestNativeDialects(form.model, channels) : null

  const num = (key: keyof typeof EMPTY_PRICE, label: string) => (
    <label>
      {label}
      <input
        type="number"
        step="0.0001"
        value={form[key] as number}
        onChange={(e) => setForm({ ...form, [key]: Number(e.target.value) })}
      />
    </label>
  )

  return (
    <div>
      <h1>模型价格</h1>
      {error && <div className="error">{error}</div>}
      <form
        className="card form-grid"
        onSubmit={async (e) => {
          e.preventDefault()
          setError('')
          const dialects = Object.fromEntries(Object.entries(native).filter(([, v]) => v))
          try {
            await api.upsertModelPrice({ ...form, native_dialects: dialects })
            setForm({ ...EMPTY_PRICE })
            setNative({})
            await refresh()
          } catch (err: any) {
            setError(err.message)
          }
        }}
      >
        <label>
          模型名（客户端请求名）
          <input
            value={form.model}
            onChange={(e) => setForm({ ...form, model: e.target.value })}
            required
          />
        </label>
        {num('model_ratio', '输入倍率')}
        {num('completion_ratio', '输出 / 输入')}
        {num('cache_read_ratio', '缓存读取 / 输入')}
        {num('cache_write_ratio', '缓存写入 / 输入')}
        {num('search_price', '每次搜索额度')}
        {NATIVE_PROTOCOLS.map((p) => (
          <label key={p}>
            原厂方言 · {PROTOCOL_LABELS[p] ?? p}
            <select value={native[p] ?? ''} onChange={(e) => setNative({ ...native, [p]: e.target.value })}>
              <option value="">standard（未配置）</option>
              {dialectsOf(meta, p)
                .filter((d) => !d.opaque && d.name !== 'standard')
                .map((d) => (
                  <option key={d.name} value={d.name} title={d.label}>
                    {d.name}
                  </option>
                ))}
            </select>
          </label>
        ))}
        {suggestion && (
          <p className="hint wide">
            建议：
            {Object.entries(suggestion.dialects)
              .map(([p, d]) => `${PROTOCOL_LABELS[p] ?? p} = ${d}`)
              .join('，')}
            （来源：{suggestion.source}）{' '}
            <button type="button" onClick={() => setNative({ ...suggestion.dialects })}>
              采用建议
            </button>
          </p>
        )}
        <label className="wide">
          输入长度分档（JSON，可留空）
          <textarea
            placeholder='[{"max_input": 32000, "model_ratio": 0.4}, {"model_ratio": 0.8, "completion_ratio": 4}]'
            value={form.tiers}
            onChange={(e) => setForm({ ...form, tiers: e.target.value })}
          />
        </label>
        <label className="wide">
          时段系数（JSON，UTC 时间，可留空）
          <textarea
            placeholder='[{"start": "16:30", "end": "00:30", "multiplier": 0.5}]'
            value={form.periods}
            onChange={(e) => setForm({ ...form, periods: e.target.value })}
          />
        </label>
        <p className="hint wide">
          推理 token 已包含在输出里，不另计费。原厂方言是客户端对该模型所用的厂商规范：同一模型无论走哪个渠道，请求按它理解，响应和 usage 按它返回。
        </p>
        <button type="submit">保存</button>
      </form>
      <table>
        <thead>
          <tr>
            <th>模型</th>
            <th>输入</th>
            <th>输出</th>
            <th>缓存读</th>
            <th>缓存写</th>
            <th>搜索</th>
            <th>原厂方言</th>
            <th>分档 / 时段</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {prices.map((p) => (
            <tr key={p.id}>
              <td>{p.model}</td>
              <td>{p.model_ratio}</td>
              <td>{p.completion_ratio}</td>
              <td>{p.cache_read_ratio}</td>
              <td>{p.cache_write_ratio}</td>
              <td>{p.search_price}</td>
              <td className="mono">
                {Object.entries(parseNative(p.native_dialects))
                  .map(([k, v]) => `${PROTOCOL_LABELS[k] ?? k}: ${v}`)
                  .join(' / ') || '-'}
              </td>
              <td className="mono">
                {[p.tiers, p.periods].filter(Boolean).join(' / ') || '-'}
              </td>
              <td>
                <button
                  onClick={() => {
                    const { native_dialects, ...rest } = p
                    setForm({ ...EMPTY_PRICE, ...rest })
                    setNative(parseNative(native_dialects))
                  }}
                >
                  编辑
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
      <h1>调用日志</h1>
      <table>
        <thead>
          <tr>
            <th>时间</th>
            <th>用户</th>
            <th>渠道</th>
            <th>模型</th>
            <th>输入</th>
            <th>缓存读 / 写</th>
            <th>输出（推理）</th>
            <th>用量格式</th>
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
              <td>{l.channel_id ?? '-'}</td>
              <td>{l.model}</td>
              <td>{l.prompt_tokens}</td>
              <td>
                {l.cache_read_tokens} / {l.cache_write_tokens}
              </td>
              <td>
                {l.completion_tokens}
                {l.reasoning_tokens > 0 ? `（${l.reasoning_tokens}）` : ''}
              </td>
              <td className="mono">{l.usage_adapter || '-'}</td>
              <td>{l.quota}</td>
              <td>{l.duration_ms}ms</td>
              <td className={`status-${l.status}`} title={l.error ?? l.raw_usage ?? ''}>
                {l.status === 'billing_failed' ? '计费失败' : l.status}
              </td>
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
