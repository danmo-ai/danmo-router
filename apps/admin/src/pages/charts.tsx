import { fmtNum } from '@/lib/format'

export { fmtNum }

const COLORS = [
  'var(--chart-1)',
  'var(--chart-2)',
  'var(--chart-3)',
  'var(--chart-4)',
  'var(--chart-5)',
]

export function BarChart({
  title,
  values,
  labels,
  color = COLORS[0],
}: {
  title: string
  values: number[]
  labels: string[]
  color?: string
}) {
  const max = Math.max(1, ...values)
  const w = Math.max(320, labels.length * 28)
  const h = 160
  const padX = 8
  const padTop = 18
  const padBottom = 8
  const plotH = h - padTop - padBottom
  const slot = (w - padX * 2) / Math.max(1, values.length)
  const barW = Math.max(6, slot - 4)
  return (
    <div className="rounded-xl border bg-card p-4">
      <div className="mb-2 text-sm font-medium">{title}</div>
      <svg viewBox={`0 0 ${w} ${h}`} className="block h-36 w-full" role="img">
        {[0, 0.5, 1].map((t) => {
          const y = padTop + plotH * (1 - t)
          return (
            <line
              key={t}
              x1={padX}
              x2={w - padX}
              y1={y}
              y2={y}
              stroke="var(--border)"
              strokeWidth="1"
            />
          )
        })}
        <text
          x={w - padX}
          y={12}
          textAnchor="end"
          className="fill-muted-foreground"
          fontSize="10"
        >
          {fmtNum(max)}
        </text>
        {values.map((v, i) => {
          const bh = (v / max) * plotH
          const x = padX + i * slot + (slot - barW) / 2
          const y = padTop + plotH - bh
          return (
            <rect key={i} x={x} y={y} width={barW} height={bh} fill={color} rx={2}>
              <title>
                {labels[i]}: {fmtNum(v)}
              </title>
            </rect>
          )
        })}
      </svg>
      <div className="mt-1 flex justify-between text-[11px] text-muted-foreground">
        {labels.length > 0 && labels.length <= 14
          ? labels.map((l) => <span key={l}>{l.slice(5)}</span>)
          : null}
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
  const legend = ['未缓存输入', '缓存读', '缓存写', '输出']
  const max = Math.max(1, ...rows.map((r) => r.parts.reduce((a, b) => a + b, 0)))
  const w = Math.max(320, rows.length * 28)
  const h = 160
  const padX = 8
  const padTop = 18
  const padBottom = 8
  const plotH = h - padTop - padBottom
  const slot = (w - padX * 2) / Math.max(1, rows.length)
  const barW = Math.max(6, slot - 4)
  return (
    <div className="rounded-xl border bg-card p-4">
      <div className="mb-2 text-sm font-medium">Token 构成（按日）</div>
      <div className="mb-2 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
        {legend.map((name, i) => (
          <span key={name} className="inline-flex items-center gap-1.5">
            <i className="inline-block size-2.5 rounded-sm" style={{ background: COLORS[i] }} />
            {name}
          </span>
        ))}
      </div>
      <svg viewBox={`0 0 ${w} ${h}`} className="block h-36 w-full" role="img">
        {[0, 0.5, 1].map((t) => {
          const y = padTop + plotH * (1 - t)
          return (
            <line
              key={t}
              x1={padX}
              x2={w - padX}
              y1={y}
              y2={y}
              stroke="var(--border)"
              strokeWidth="1"
            />
          )
        })}
        <text
          x={w - padX}
          y={12}
          textAnchor="end"
          className="fill-muted-foreground"
          fontSize="10"
        >
          {fmtNum(max)}
        </text>
        {rows.map((r, i) => {
          const x = padX + i * slot + (slot - barW) / 2
          let y = padTop + plotH
          const total = r.parts.reduce((a, b) => a + b, 0)
          return r.parts.map((p, j) => {
            const bh = (p / max) * plotH
            y -= bh
            return (
              <rect key={`${i}-${j}`} x={x} y={y} width={barW} height={bh} fill={COLORS[j]}>
                <title>
                  {r.day} · {legend[j]}: {fmtNum(p)}（合计 {fmtNum(total)}）
                </title>
              </rect>
            )
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
    <div className="rounded-xl border bg-card p-4">
      <div className="mb-3 text-sm font-medium">消耗额度 · 模型 Top</div>
      {rows.length === 0 ? (
        <p className="text-sm text-muted-foreground">暂无数据</p>
      ) : (
        <div className="flex flex-col gap-3">
          {rows.map((r) => (
            <div key={r.model}>
              <div className="mb-1 flex justify-between gap-3 text-xs">
                <span className="truncate">{r.model}</span>
                <span className="shrink-0 text-muted-foreground tabular-nums">
                  {fmtNum(r.quota)} · {fmtNum(r.requests)} 次
                </span>
              </div>
              <div className="h-1.5 overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full rounded-full bg-chart-1"
                  style={{ width: `${(r.quota / max) * 100}%` }}
                />
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
