import { useEffect, useMemo, useState } from 'react'
import type { FormEvent } from 'react'
import { api } from '../api'
import type { UsageReport } from '../api'
import { PAGE_SIZE } from '../shared'
import { BarChart, ModelBars, StackedTokenChart, fmtNum } from './charts'
import { DataTable, EmptyRow, Pager, SkeletonRows } from '@/components/data-table'
import { ErrorAlert } from '@/components/error-alert'
import { Field } from '@/components/field'
import { PageHeader } from '@/components/page-header'
import { StatCard } from '@/components/stat-card'
import { StatusBadge } from '@/components/status-badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { cn } from 'cn'

type LogFilters = {
  start: string
  end: string
  model: string
  token_id: string
  status: string
  user_id: string
  channel_id: string
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
    user_id: '',
    channel_id: '',
  }
}

function filterParams(f: LogFilters) {
  return {
    start: f.start || undefined,
    end: f.end || undefined,
    model: f.model || undefined,
    token_id: f.token_id ? Number(f.token_id) : undefined,
    status: f.status || undefined,
    user_id: f.user_id ? Number(f.user_id) : undefined,
    channel_id: f.channel_id ? Number(f.channel_id) : undefined,
  }
}

const ANY = '__any__'

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
    const params = { ...filterParams(filters), limit: PAGE_SIZE, offset }
    Promise.all([api.usage(filterParams(filters)), api.logs(params)])
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
      <PageHeader title="调用日志" description="按日汇总，以及保留期内的逐笔记录。时间均为 UTC。" />
      <form className="mb-4 flex flex-wrap items-end gap-3 rounded-xl border bg-card p-3" onSubmit={applyDraft}>
        <div className="inline-flex h-9 rounded-lg border bg-muted p-0.5">
          <PresetButton active={filters.start === daysAgo(6)} onClick={() => applyPreset(6)}>
            近 7 天
          </PresetButton>
          <PresetButton active={filters.start === daysAgo(29)} onClick={() => applyPreset(29)}>
            近 30 天
          </PresetButton>
        </div>
        <Field label="起（UTC）" className="w-36">
          <Input type="date" value={draft.start} onChange={(e) => setDraft({ ...draft, start: e.target.value })} />
        </Field>
        <Field label="止（UTC）" className="w-36">
          <Input type="date" value={draft.end} onChange={(e) => setDraft({ ...draft, end: e.target.value })} />
        </Field>
        <Field label="模型" className="w-36">
          <Input
            value={draft.model}
            onChange={(e) => setDraft({ ...draft, model: e.target.value })}
            placeholder="可选"
          />
        </Field>
        <Field label="令牌 ID" className="w-28">
          <Input
            value={draft.token_id}
            onChange={(e) => setDraft({ ...draft, token_id: e.target.value })}
            placeholder="可选"
          />
        </Field>
        <Field label="用户 ID" className="w-28">
          <Input
            value={draft.user_id}
            onChange={(e) => setDraft({ ...draft, user_id: e.target.value })}
            placeholder="可选"
          />
        </Field>
        <Field label="渠道 ID" className="w-28">
          <Input
            value={draft.channel_id}
            onChange={(e) => setDraft({ ...draft, channel_id: e.target.value })}
            placeholder="可选"
          />
        </Field>
        <Field label="状态" className="w-32">
          <Select
            value={draft.status || ANY}
            onValueChange={(status) => setDraft({ ...draft, status: status === ANY ? '' : status })}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ANY}>全部</SelectItem>
              <SelectItem value="success">成功</SelectItem>
              <SelectItem value="error">错误</SelectItem>
              <SelectItem value="billing_failed">计费失败</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        <Button type="submit">查询</Button>
      </form>

      {error ? <div className="mb-4"><ErrorAlert>{error}</ErrorAlert></div> : null}

      {usage ? (
        <>
          <div className="mb-4 grid gap-3 sm:grid-cols-3">
            <StatCard label="请求次数" value={usage.totals.requests} />
            <StatCard
              label="Token 总数"
              value={tokenTotal}
              hint={
                usage.totals.reasoning_tokens > 0
                  ? `含推理 ${fmtNum(usage.totals.reasoning_tokens)}`
                  : undefined
              }
            />
            <StatCard label="消耗额度" value={usage.totals.quota} />
          </div>
          <div className="mb-6 grid gap-3 lg:grid-cols-2">
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
              color="var(--chart-2)"
            />
            <ModelBars rows={usage.by_model} />
          </div>
        </>
      ) : loading ? (
        <p className="mb-4 text-sm text-muted-foreground">加载中...</p>
      ) : null}

      <h2 className="mb-1 text-base font-semibold">逐笔记录</h2>
      <p className="mb-3 text-xs text-muted-foreground">
        逐笔记录保留最近 {usage?.retention_days ?? '—'} 天，更早的只在上方图表中。
      </p>
      <DataTable>
        <table className="w-full caption-bottom text-sm">
          <TableHeader>
            <TableRow>
              <TableHead>时间</TableHead>
              <TableHead>用户</TableHead>
              <TableHead>渠道</TableHead>
              <TableHead>模型</TableHead>
              <TableHead className="text-right">输入</TableHead>
              <TableHead className="text-right">缓存读 / 写</TableHead>
              <TableHead className="text-right">输出（推理）</TableHead>
              <TableHead>用量格式</TableHead>
              <TableHead className="text-right">额度</TableHead>
              <TableHead className="text-right">耗时</TableHead>
              <TableHead>状态</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {loading && logs.length === 0 ? (
              <SkeletonRows cols={11} />
            ) : logs.length === 0 ? (
              <EmptyRow colSpan={11}>所选范围内没有逐笔记录</EmptyRow>
            ) : (
              logs.map((l) => (
                <TableRow key={l.id}>
                  <TableCell className="font-mono text-xs">{l.created_at}</TableCell>
                  <TableCell className="tabular-nums">{l.user_id}</TableCell>
                  <TableCell className="tabular-nums">{l.channel_id ?? '-'}</TableCell>
                  <TableCell>{l.model}</TableCell>
                  <TableCell className="text-right tabular-nums">{fmtNum(l.prompt_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {fmtNum(l.cache_read_tokens)} / {fmtNum(l.cache_write_tokens)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {fmtNum(l.completion_tokens)}
                    {l.reasoning_tokens > 0 ? `（${fmtNum(l.reasoning_tokens)}）` : ''}
                  </TableCell>
                  <TableCell className="font-mono text-xs">{l.usage_adapter || '-'}</TableCell>
                  <TableCell className="text-right tabular-nums">{fmtNum(l.quota)}</TableCell>
                  <TableCell className="text-right tabular-nums">{l.duration_ms}ms</TableCell>
                  <TableCell>
                    <span title={l.error ?? l.raw_usage ?? ''}>
                      <StatusBadge status={l.status} />
                    </span>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </table>
      </DataTable>
      <Pager
        page={offset / PAGE_SIZE + 1}
        hasPrev={offset > 0}
        hasNext={logs.length >= PAGE_SIZE}
        onPrev={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
        onNext={() => setOffset(offset + PAGE_SIZE)}
      />
    </div>
  )
}

function PresetButton({
  active,
  onClick,
  children,
}: {
  active: boolean
  onClick: () => void
  children: string
}) {
  return (
    <button
      type="button"
      className={cn(
        'rounded-md px-3 text-sm text-muted-foreground',
        active && 'bg-background font-medium text-foreground shadow-sm',
      )}
      onClick={onClick}
    >
      {children}
    </button>
  )
}
