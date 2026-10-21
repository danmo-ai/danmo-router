import { useEffect, useMemo, useState } from 'react'
import type { FormEvent } from 'react'
import { api } from '../api'
import type { UsageReport } from '../api'
import { PAGE_SIZE, statusLabel } from '../shared'
import { BarChart, ModelBars, StackedTokenChart, fmtNum } from './charts'

type LogFilters = {
  start: string
  end: string
  model: string
  token_id: string
  status: string
}

function utcDay(d: Date = new Date()) {
  return d.toISOString().slice(0, 10)
}

function daysAgo(n: number) {
  const d = new Date()
  d.setUTCDate(d.getUTCDate() - n)
  return utcDay(d)
}

function defaultFilters(days = 6): LogFilters {
  return {
    start: daysAgo(days),
    end: utcDay(),
    model: '',
    token_id: '',
    status: '',
  }
}

function filterParams(f: LogFilters) {
  return {
    start: f.start || undefined,
    end: f.end || undefined,
    model: f.model || undefined,
    token_id: f.token_id ? Number(f.token_id) : undefined,
    status: f.status || undefined,
  }
}

export function LogsPage() {
  const [filters, setFilters] = useState<LogFilters>(() => defaultFilters(6))
  const [draft, setDraft] = useState<LogFilters>(() => defaultFilters(6))
  const [logs, setLogs] = useState<any[]>([])
  const [usage, setUsage] = useState<UsageReport | null>(null)
  const [offset, setOffset] = useState(0)
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError('')
    Promise.all([
      api.usage(filterParams(filters)),
      api.logs({ ...filterParams(filters), limit: PAGE_SIZE, offset }),
    ])
      .then(([u, l]) => {
        if (cancelled) return
        setUsage(u)
        setLogs(l)
      })
      .catch((err: any) => {
        if (!cancelled) setError(err.message || String(err))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [filters, offset])

  const tokenTotal = useMemo(() => {
    if (!usage) return 0
    return usage.totals.prompt_tokens + usage.totals.completion_tokens
  }, [usage])

  function applyPreset(days: number) {
    const next = defaultFilters(days)
    setDraft(next)
    setFilters(next)
    setOffset(0)
  }

  function applyDraft(e: FormEvent) {
    e.preventDefault()
    setFilters({ ...draft })
    setOffset(0)
  }

  return (
    <div>
      <h1>我的调用日志</h1>
      <form className="filter-bar" onSubmit={applyDraft}>
        <div className="filter-presets">
          <button
            type="button"
            className={filters.start === daysAgo(6) ? 'active' : ''}
            onClick={() => applyPreset(6)}
          >
            近 7 天
          </button>
          <button
            type="button"
            className={filters.start === daysAgo(29) ? 'active' : ''}
            onClick={() => applyPreset(29)}
          >
            近 30 天
          </button>
        </div>
        <label>
          起（UTC）
          <input
            type="date"
            value={draft.start}
            onChange={(e) => setDraft({ ...draft, start: e.target.value })}
          />
        </label>
        <label>
          止（UTC）
          <input
            type="date"
            value={draft.end}
            onChange={(e) => setDraft({ ...draft, end: e.target.value })}
          />
        </label>
        <label>
          模型
          <input
            value={draft.model}
            onChange={(e) => setDraft({ ...draft, model: e.target.value })}
            placeholder="可选"
          />
        </label>
        <label>
          令牌 ID
          <input
            value={draft.token_id}
            onChange={(e) => setDraft({ ...draft, token_id: e.target.value })}
            placeholder="可选"
          />
        </label>
        <label>
          状态
          <select value={draft.status} onChange={(e) => setDraft({ ...draft, status: e.target.value })}>
            <option value="">全部</option>
            <option value="success">成功</option>
            <option value="error">错误</option>
            <option value="billing_failed">计费失败</option>
          </select>
        </label>
        <button type="submit">查询</button>
      </form>

      {error && <div className="error">{error}</div>}
      {loading && !usage ? <p className="muted">加载中...</p> : null}

      {usage && (
        <>
          <div className="grid">
            <div className="stat">
              <div className="label">请求次数</div>
              <div className="value">{fmtNum(usage.totals.requests)}</div>
            </div>
            <div className="stat">
              <div className="label">Token 总数</div>
              <div className="value">{fmtNum(tokenTotal)}</div>
            </div>
            <div className="stat">
              <div className="label">消耗额度</div>
              <div className="value">{fmtNum(usage.totals.quota)}</div>
            </div>
          </div>
          <div className="chart-grid">
            <BarChart
              title="请求次数（按日）"
              values={usage.series.map((s) => s.requests)}
              labels={usage.series.map((s) => s.day)}
            />
            <StackedTokenChart series={usage.series} />
            <BarChart
              title="消耗额度（按日）"
              values={usage.series.map((s) => s.quota)}
              labels={usage.series.map((s) => s.day)}
              color="#0d9488"
            />
            <ModelBars rows={usage.by_model} />
          </div>
        </>
      )}

      <h2>逐笔记录</h2>
      <p className="hint">
        逐笔记录保留最近 {usage?.retention_days ?? '—'} 天，更早的只在上方图表中。时间均为 UTC。
      </p>
      <table>
        <thead>
          <tr>
            <th>时间</th>
            <th>模型</th>
            <th>输入</th>
            <th>输出</th>
            <th>额度</th>
            <th>状态</th>
          </tr>
        </thead>
        <tbody>
          {logs.length === 0 ? (
            <tr>
              <td colSpan={6} className="muted">
                所选范围内没有逐笔记录
              </td>
            </tr>
          ) : (
            logs.map((l) => (
              <tr key={l.id}>
                <td>{l.created_at}</td>
                <td>{l.model}</td>
                <td>{l.prompt_tokens}</td>
                <td>{l.completion_tokens}</td>
                <td>{l.quota}</td>
                <td className={`status-${l.status}`}>{statusLabel(l.status)}</td>
              </tr>
            ))
          )}
        </tbody>
      </table>
      <div className="pager">
        <button type="button" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}>
          上一页
        </button>
        <span>第 {offset / PAGE_SIZE + 1} 页</span>
        <button type="button" disabled={logs.length < PAGE_SIZE} onClick={() => setOffset(offset + PAGE_SIZE)}>
          下一页
        </button>
      </div>
    </div>
  )
}
