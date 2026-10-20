import { useEffect, useState } from 'react'
import { api } from '../api'
import { PROTOCOL_LABELS, suggestNativeDialects } from '../presets'
import { dialectsOf, useMeta } from '../shared'

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

const NATIVE_PROTOCOLS = ['openai_chat', 'openai_responses', 'anthropic_messages']

function parseNative(raw: string): Record<string, string> {
  try {
    const v = JSON.parse(raw || '{}')
    return v && typeof v === 'object' && !Array.isArray(v) ? v : {}
  } catch {
    return {}
  }
}

export function PricesPage() {
  const meta = useMeta()
  const [prices, setPrices] = useState<any[]>([])
  const [channels, setChannels] = useState<any[]>([])
  const [form, setForm] = useState({ ...EMPTY_PRICE })
  const [native, setNative] = useState<Record<string, string>>({})
  const [error, setError] = useState('')
  const [ratios, setRatios] = useState<any[]>([])
  const [groupName, setGroupName] = useState('default')
  const [groupRatio, setGroupRatio] = useState(1)

  async function refresh() {
    setPrices(await api.modelPrices())
    setRatios(await api.groupRatios())
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
          推理 token 已包含在输出里，不另计费。原厂方言是客户端对该模型所用的厂商规范：同一模型无论走哪个渠道，请求按它理解，响应和 usage 按它返回。计费再乘分组倍率。
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

      <h2>分组倍率</h2>
      <form
        className="card form-grid"
        onSubmit={async (e) => {
          e.preventDefault()
          setError('')
          try {
            await api.upsertGroupRatio(groupName.trim(), groupRatio)
            await refresh()
          } catch (err: any) {
            setError(err.message)
          }
        }}
      >
        <input
          placeholder="分组名"
          value={groupName}
          onChange={(e) => setGroupName(e.target.value)}
          required
        />
        <input
          type="number"
          step="0.0001"
          min={0}
          placeholder="倍率"
          value={groupRatio}
          onChange={(e) => setGroupRatio(Number(e.target.value))}
          required
        />
        <button type="submit">保存分组</button>
      </form>
      <table>
        <thead>
          <tr>
            <th>分组</th>
            <th>倍率</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {ratios.map((r) => (
            <tr key={r.id}>
              <td>{r.group_name}</td>
              <td>{r.ratio}</td>
              <td>
                <button
                  onClick={() => {
                    setGroupName(r.group_name)
                    setGroupRatio(r.ratio)
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
