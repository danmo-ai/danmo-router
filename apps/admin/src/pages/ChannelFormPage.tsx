import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { api } from '../api'
import { CHANNEL_PRESETS, MODE_LABELS, PROTOCOL_LABELS, dialectForModel } from '../presets'
import type { DialectRule } from '../presets'
import { DialectSelect, dialectsOf, useMeta } from '../shared'

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

export function ChannelFormPage() {
  const { id } = useParams()
  const editing = id ? Number(id) : null
  const navigate = useNavigate()
  const meta = useMeta()
  const [form, setForm] = useState({ ...EMPTY_CHANNEL })
  const [rows, setRows] = useState<ModelRow[]>([])
  const [rules, setRules] = useState<DialectRule[]>([])
  const [newModels, setNewModels] = useState(editing === null ? 'gpt-4o-mini' : '')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(editing !== null)

  useEffect(() => {
    if (editing === null) return
    let cancelled = false
    api
      .channels()
      .then((list) => {
        if (cancelled) return
        const c = list.find((x: any) => x.id === editing)
        if (!c) {
          setError('渠道不存在')
          setLoading(false)
          return
        }
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
        setLoading(false)
      })
      .catch((err: any) => {
        if (!cancelled) {
          setError(err.message)
          setLoading(false)
        }
      })
    return () => {
      cancelled = true
    }
  }, [editing])

  function applyPreset(presetId: string) {
    const p = CHANNEL_PRESETS.find((x) => x.id === presetId)
    if (!p) return
    setForm((prev) => ({
      ...prev,
      name: prev.name || p.label,
      type: p.type,
      vendor: p.vendor,
      base_url: p.base_url,
      upstream_protocol: p.upstream_protocol,
      auth_style: p.auth_style,
      default_dialect: p.default_dialect,
    }))
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
      } else {
        await api.createChannel(body)
      }
      navigate('/channels')
    } catch (err: any) {
      setError(err.message)
    }
  }

  const groups = [...new Set(CHANNEL_PRESETS.map((p) => p.group))]
  const canConvert = form.upstream_protocol !== 'embeddings'

  if (loading) return <p className="muted">加载中...</p>

  return (
    <div className="form-page">
      <div className="page-header">
        <div>
          <Link className="back-link" to="/channels">
            ← 返回渠道列表
          </Link>
          <h1>{editing !== null ? '编辑渠道' : '新增渠道'}</h1>
        </div>
      </div>
      {error && <div className="error">{error}</div>}
      <form className="card form-stack" onSubmit={submit}>
        {editing === null && (
          <label>
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
        )}
        <div className="form-cols-2">
          <label>
            名称
            <input
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              required
            />
          </label>
          <label>
            Base URL
            <input
              value={form.base_url}
              onChange={(e) => setForm({ ...form, base_url: e.target.value })}
              required
            />
          </label>
        </div>
        <label>
          上游 API Key{editing !== null ? '（留空不改）' : ''}
          <input
            value={form.key}
            onChange={(e) => setForm({ ...form, key: e.target.value })}
            required={editing === null}
          />
        </label>
        <div className="form-cols-3">
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
        </div>
        <div className="form-cols-3">
          <label>
            新模型默认方言
            <DialectSelect
              meta={meta}
              protocol={form.upstream_protocol}
              value={form.default_dialect}
              onChange={(v) => setForm({ ...form, default_dialect: v })}
            />
          </label>
          <label>
            原厂标识（vendor，聚合渠道留空）
            <input
              value={form.vendor}
              onChange={(e) => setForm({ ...form, vendor: e.target.value })}
            />
          </label>
          <label>
            分组
            <input
              value={form.group_name}
              onChange={(e) => setForm({ ...form, group_name: e.target.value })}
            />
          </label>
        </div>
        <div className="form-cols-2">
          <label>
            优先级
            <input
              type="number"
              value={form.priority}
              onChange={(e) => setForm({ ...form, priority: Number(e.target.value) })}
            />
          </label>
          <label>
            权重
            <input
              type="number"
              value={form.weight}
              onChange={(e) => setForm({ ...form, weight: Number(e.target.value) })}
            />
          </label>
        </div>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={form.inject_stream_usage}
            onChange={(e) => setForm({ ...form, inject_stream_usage: e.target.checked })}
          />
          流式请求补 include_usage（计费需要）
        </label>
        <div>
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
        <p className="hint">
          方言是上游对该模型实际遵循的厂商规范。客户端协议与该模型的原厂方言都和这一行一致时原样透传，否则经中间表示转换；opaque（聚合渠道）只透传同协议请求。
          转发模式只决定能否服务其他协议。权重在同一优先级内按比例分配流量。
        </p>
        <div className="form-actions">
          <button type="submit">保存</button>
          <Link className="button ghost" to="/channels">
            取消
          </Link>
        </div>
      </form>
    </div>
  )
}
