import { useEffect, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { toast } from 'sonner'
import { api } from '../api'
import { PROTOCOL_LABELS, suggestNativeDialects } from '../presets'
import { dialectsOf, useMeta } from '../shared'
import { DataTable, EmptyRow, SkeletonRows } from '@/components/data-table'
import { ErrorAlert } from '@/components/error-alert'
import { Field } from '@/components/field'
import { PageHeader } from '@/components/page-header'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
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
import { Textarea } from '@/components/ui/textarea'

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
const NONE = '__standard__'

function parseNative(raw: string): Record<string, string> {
  try {
    const v = JSON.parse(raw || '{}')
    return v && typeof v === 'object' && !Array.isArray(v) ? v : {}
  } catch {
    return {}
  }
}

export function PricesPage() {
  const [prices, setPrices] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [ratios, setRatios] = useState<any[]>([])
  const [groupName, setGroupName] = useState('default')
  const [groupRatio, setGroupRatio] = useState(1)

  async function refresh() {
    setPrices(await api.modelPrices())
    setRatios(await api.groupRatios())
  }
  useEffect(() => {
    refresh()
      .catch(console.error)
      .finally(() => setLoading(false))
  }, [])

  return (
    <div>
      <PageHeader
        title="模型价格"
        description="输入倍率、缓存和原厂方言。计费再乘分组倍率。"
        actions={
          <Button asChild>
            <Link to="/prices/new">新增价格</Link>
          </Button>
        }
      />
      {error ? <div className="mb-4"><ErrorAlert>{error}</ErrorAlert></div> : null}
      <DataTable>
        <table className="w-full caption-bottom text-sm">
          <TableHeader>
            <TableRow>
              <TableHead>模型</TableHead>
              <TableHead>输入</TableHead>
              <TableHead>输出</TableHead>
              <TableHead>缓存读</TableHead>
              <TableHead>缓存写</TableHead>
              <TableHead>搜索</TableHead>
              <TableHead>原厂方言</TableHead>
              <TableHead>分档 / 时段</TableHead>
              <TableHead className="text-right" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {loading ? (
              <SkeletonRows cols={9} />
            ) : prices.length === 0 ? (
              <EmptyRow colSpan={9}>还没有价格</EmptyRow>
            ) : (
              prices.map((p) => (
                <TableRow key={p.id}>
                  <TableCell className="font-medium">{p.model}</TableCell>
                  <TableCell className="tabular-nums">{p.model_ratio}</TableCell>
                  <TableCell className="tabular-nums">{p.completion_ratio}</TableCell>
                  <TableCell className="tabular-nums">{p.cache_read_ratio}</TableCell>
                  <TableCell className="tabular-nums">{p.cache_write_ratio}</TableCell>
                  <TableCell className="tabular-nums">{p.search_price}</TableCell>
                  <TableCell className="max-w-xs font-mono text-xs whitespace-normal">
                    {Object.entries(parseNative(p.native_dialects))
                      .map(([k, v]) => `${PROTOCOL_LABELS[k] ?? k}: ${v}`)
                      .join(' / ') || '-'}
                  </TableCell>
                  <TableCell className="max-w-xs font-mono text-xs whitespace-normal">
                    {[p.tiers, p.periods].filter(Boolean).join(' / ') || '-'}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button variant="ghost" size="sm" asChild>
                      <Link to={`/prices/${encodeURIComponent(p.model)}/edit`}>编辑</Link>
                    </Button>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </table>
      </DataTable>

      <h2 className="mt-8 mb-3 text-base font-semibold">分组倍率</h2>
      <Card className="mb-4 shadow-none">
        <CardContent className="pt-6">
          <form
            className="grid gap-3 sm:grid-cols-[1fr_160px_auto]"
            onSubmit={async (e) => {
              e.preventDefault()
              setError('')
              try {
                await api.upsertGroupRatio(groupName.trim(), groupRatio)
                toast.success('分组倍率已保存')
                await refresh()
              } catch (err: any) {
                setError(err.message)
              }
            }}
          >
            <Input
              placeholder="分组名"
              value={groupName}
              onChange={(e) => setGroupName(e.target.value)}
              required
            />
            <Input
              type="number"
              step="0.0001"
              min={0}
              placeholder="倍率"
              value={groupRatio}
              onChange={(e) => setGroupRatio(Number(e.target.value))}
              required
            />
            <Button type="submit">保存</Button>
          </form>
        </CardContent>
      </Card>
      <DataTable>
        <table className="w-full caption-bottom text-sm">
          <TableHeader>
            <TableRow>
              <TableHead>分组</TableHead>
              <TableHead>倍率</TableHead>
              <TableHead className="text-right" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {ratios.length === 0 ? (
              <EmptyRow colSpan={3}>还没有分组倍率</EmptyRow>
            ) : (
              ratios.map((r) => (
                <TableRow key={r.id}>
                  <TableCell>{r.group_name}</TableCell>
                  <TableCell className="tabular-nums">{r.ratio}</TableCell>
                  <TableCell className="text-right">
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        setGroupName(r.group_name)
                        setGroupRatio(r.ratio)
                      }}
                    >
                      编辑
                    </Button>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </table>
      </DataTable>
    </div>
  )
}

export function PriceFormPage() {
  const { model: modelParam } = useParams()
  const editingModel = modelParam ? decodeURIComponent(modelParam) : null
  const navigate = useNavigate()
  const meta = useMeta()
  const [channels, setChannels] = useState<any[]>([])
  const [form, setForm] = useState({ ...EMPTY_PRICE })
  const [native, setNative] = useState<Record<string, string>>({})
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(editingModel !== null)

  useEffect(() => {
    api.channels().then(setChannels).catch(console.error)
  }, [])

  useEffect(() => {
    if (!editingModel) return
    let cancelled = false
    api
      .modelPrices()
      .then((list) => {
        if (cancelled) return
        const p = list.find((x: any) => x.model === editingModel)
        if (!p) {
          setError('价格不存在')
          setLoading(false)
          return
        }
        const { native_dialects, ...rest } = p
        setForm({ ...EMPTY_PRICE, ...rest })
        setNative(parseNative(native_dialects))
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
  }, [editingModel])

  const suggestion = form.model ? suggestNativeDialects(form.model, channels) : null

  const num = (key: keyof typeof EMPTY_PRICE, label: string) => (
    <Field label={label}>
      <Input
        type="number"
        step="0.0001"
        value={form[key] as number}
        onChange={(e) => setForm({ ...form, [key]: Number(e.target.value) })}
      />
    </Field>
  )

  if (loading) return <p className="text-sm text-muted-foreground">加载中...</p>

  return (
    <div>
      <PageHeader
        eyebrow={
          <Link className="text-xs text-muted-foreground hover:text-foreground" to="/prices">
            ← 返回价格列表
          </Link>
        }
        title={editingModel ? `编辑价格 · ${editingModel}` : '新增价格'}
      />
      {error ? <div className="mb-4"><ErrorAlert>{error}</ErrorAlert></div> : null}
      <form
        className="grid max-w-4xl gap-4"
        onSubmit={async (e) => {
          e.preventDefault()
          setError('')
          const dialects = Object.fromEntries(Object.entries(native).filter(([, v]) => v))
          try {
            await api.upsertModelPrice({ ...form, native_dialects: dialects })
            navigate('/prices')
          } catch (err: any) {
            setError(err.message)
          }
        }}
      >
        <Card className="shadow-none">
          <CardHeader>
            <CardTitle>倍率</CardTitle>
          </CardHeader>
          <CardContent className="grid gap-4">
            <Field label="模型名（客户端请求名）">
              <Input
                value={form.model}
                onChange={(e) => setForm({ ...form, model: e.target.value })}
                required
                disabled={editingModel !== null}
              />
            </Field>
            <div className="grid gap-4 sm:grid-cols-3">
              {num('model_ratio', '输入倍率')}
              {num('completion_ratio', '输出 / 输入')}
              {num('cache_read_ratio', '缓存读取 / 输入')}
              {num('cache_write_ratio', '缓存写入 / 输入')}
              {num('search_price', '每次搜索额度')}
            </div>
          </CardContent>
        </Card>
        <Card className="shadow-none">
          <CardHeader>
            <CardTitle>原厂方言</CardTitle>
          </CardHeader>
          <CardContent className="grid gap-4">
            <div className="grid gap-4 sm:grid-cols-3">
              {NATIVE_PROTOCOLS.map((p) => (
                <Field key={p} label={PROTOCOL_LABELS[p] ?? p}>
                  <Select
                    value={native[p] || NONE}
                    onValueChange={(value) =>
                      setNative({ ...native, [p]: value === NONE ? '' : value })
                    }
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value={NONE}>standard（未配置）</SelectItem>
                      {dialectsOf(meta, p)
                        .filter((d) => !d.opaque && d.name !== 'standard')
                        .map((d) => (
                          <SelectItem key={d.name} value={d.name} title={d.label}>
                            {d.name}
                          </SelectItem>
                        ))}
                    </SelectContent>
                  </Select>
                </Field>
              ))}
            </div>
            {suggestion ? (
              <p className="text-xs text-muted-foreground">
                建议：
                {Object.entries(suggestion.dialects)
                  .map(([p, d]) => `${PROTOCOL_LABELS[p] ?? p} = ${d}`)
                  .join('，')}
                （来源：{suggestion.source}）{' '}
                <Button type="button" variant="link" className="h-auto px-0" onClick={() => setNative({ ...suggestion.dialects })}>
                  采用建议
                </Button>
              </p>
            ) : null}
            <Field label="输入长度分档（JSON，可留空）">
              <Textarea
                placeholder='[{"max_input": 32000, "model_ratio": 0.4}, {"model_ratio": 0.8, "completion_ratio": 4}]'
                value={form.tiers}
                onChange={(e) => setForm({ ...form, tiers: e.target.value })}
                className="min-h-20 font-mono text-xs"
              />
            </Field>
            <Field label="时段系数（JSON，UTC 时间，可留空）">
              <Textarea
                placeholder='[{"start": "16:30", "end": "00:30", "multiplier": 0.5}]'
                value={form.periods}
                onChange={(e) => setForm({ ...form, periods: e.target.value })}
                className="min-h-20 font-mono text-xs"
              />
            </Field>
            <p className="text-xs leading-relaxed text-muted-foreground">
              推理 token 已包含在输出里，不另计费。原厂方言是客户端对该模型所用的厂商规范：同一模型无论走哪个渠道，请求按它理解，响应和 usage 按它返回。计费再乘分组倍率。
            </p>
          </CardContent>
        </Card>
        <div className="flex gap-2">
          <Button type="submit">保存</Button>
          <Button variant="outline" asChild>
            <Link to="/prices">取消</Link>
          </Button>
        </div>
      </form>
    </div>
  )
}
