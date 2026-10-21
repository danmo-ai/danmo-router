export function fmtNum(n: number) {
  return new Intl.NumberFormat('zh-CN').format(n)
}

export function BarChart({
  title,
  values,
  labels,
  color = '#2563eb',
}: {
  title: string
  values: number[]
  labels: string[]
  color?: string
}) {
  const max = Math.max(1, ...values)
  const w = Math.max(320, labels.length * 28)
  const h = 140
  const pad = 24
  const barW = Math.max(6, (w - pad * 2) / Math.max(1, values.length) - 4)
  return (
    <div className="chart-card">
      <div className="chart-title">{title}</div>
      <svg viewBox={`0 0 ${w} ${h}`} className="chart-svg" role="img">
        {values.map((v, i) => {
          const bh = (v / max) * (h - pad * 1.5)
          const x = pad + i * ((w - pad * 2) / values.length)
          const y = h - pad - bh
          return <rect key={i} x={x} y={y} width={barW} height={bh} fill={color} rx={2} />
        })}
      </svg>
      <div className="chart-x">
        {labels.length <= 14 ? labels.map((l) => <span key={l}>{l.slice(5)}</span>) : null}
      </div>
    </div>
  )
}

export function StackedTokenChart({
  series,
}: {
  series: {
    day: string
    prompt_tokens: number
    completion_tokens: number
    cache_read_tokens: number
    cache_write_tokens: number
  }[]
}) {
  const rows = series.map((s) => {
    const uncached = Math.max(0, s.prompt_tokens - s.cache_read_tokens - s.cache_write_tokens)
    return {
      day: s.day,
      parts: [uncached, s.cache_read_tokens, s.cache_write_tokens, s.completion_tokens],
    }
  })
  const colors = ['#2563eb', '#06b6d4', '#8b5cf6', '#f59e0b']
  const max = Math.max(1, ...rows.map((r) => r.parts.reduce((a, b) => a + b, 0)))
  const w = Math.max(320, rows.length * 28)
  const h = 140
  const pad = 24
  const barW = Math.max(6, (w - pad * 2) / Math.max(1, rows.length) - 4)
  return (
    <div className="chart-card">
      <div className="chart-title">Token 构成（按日）</div>
      <div className="chart-legend">
        <span>
          <i style={{ background: colors[0] }} />
          未缓存输入
        </span>
        <span>
          <i style={{ background: colors[1] }} />
          缓存读
        </span>
        <span>
          <i style={{ background: colors[2] }} />
          缓存写
        </span>
        <span>
          <i style={{ background: colors[3] }} />
          输出
        </span>
      </div>
      <svg viewBox={`0 0 ${w} ${h}`} className="chart-svg" role="img">
        {rows.map((r, i) => {
          const x = pad + i * ((w - pad * 2) / rows.length)
          let y = h - pad
          return r.parts.map((p, j) => {
            const bh = (p / max) * (h - pad * 1.5)
            y -= bh
            return <rect key={`${i}-${j}`} x={x} y={y} width={barW} height={bh} fill={colors[j]} />
          })
        })}
      </svg>
    </div>
  )
}

export function ModelBars({
  rows,
}: {
  rows: { model: string; quota: number; requests: number }[]
}) {
  const max = Math.max(1, ...rows.map((r) => r.quota))
  return (
    <div className="chart-card">
      <div className="chart-title">消耗额度 · 模型 Top</div>
      {rows.length === 0 ? (
        <p className="muted">暂无数据</p>
      ) : (
        <div className="model-bars">
          {rows.map((r) => (
            <div key={r.model} className="model-bar-row">
              <div className="model-bar-label">
                <span>{r.model}</span>
                <span className="muted">
                  {fmtNum(r.quota)} · {fmtNum(r.requests)} 次
                </span>
              </div>
              <div className="model-bar-track">
                <div className="model-bar-fill" style={{ width: `${(r.quota / max) * 100}%` }} />
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
