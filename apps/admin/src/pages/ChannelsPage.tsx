import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
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

export function ChannelsPage() {
  const meta = useMeta()
  const [channels, setChannels] = useState<any[]>([])
  const [form, setForm] = useState({ ...EMPTY_CHANNEL })
  const [rows, setRows] = useState<ModelRow[]>([])
  const [rules, setRules] = useState<DialectRule[]>([])
  const [newModels, setNewModels] = useState('gpt-4o-mini')
  const [editing, setEditing] = useState<number | null>(null)
  const [error, setError] = useState('')
  const [probe, setProbe] = useState('')

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
      {probe && <p className="hint">{probe}</p>}
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
        <input
          type="number"
          placeholder="权重"
          value={form.weight}
          onChange={(e) => setForm({ ...form, weight: Number(e.target.value) })}
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
          转发模式只决定能否服务其他协议。权重在同一优先级内按比例分配流量。
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
            <th>权重</th>
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
              <td>{c.weight}</td>
              <td>{c.status}</td>
              <td className="actions">
                <button onClick={() => edit(c)}>编辑</button>
                <button
                  onClick={async () => {
                    setProbe(`正在测试渠道 ${c.id}…`)
                    try {
                      const r = await api.testChannel(c.id)
                      setProbe(
                        r.ok
                          ? `渠道 ${c.name} 连通 ${r.status}，${r.latency_ms}ms`
                          : `渠道 ${c.name} 失败 ${r.status || '网络错误'} ${r.error || ''}（${r.latency_ms}ms）`,
                      )
                    } catch (err: any) {
                      setProbe(err.message)
                    }
                  }}
                >
                  测试
                </button>
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
