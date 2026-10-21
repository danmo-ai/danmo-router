import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { api } from '../api'
import { CHANNEL_PRESETS, MODE_LABELS, PROTOCOL_LABELS, dialectForModel } from '../presets'
import type { DialectRule } from '../presets'
import { DialectSelect, dialectsOf, useMeta } from '../shared'
import { ErrorAlert } from '@/components/error-alert'
import { Field } from '@/components/field'
import { PageHeader } from '@/components/page-header'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

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
  const [preset, setPreset] = useState('')
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

  if (loading) return <p className="text-sm text-muted-foreground">加载中...</p>

  return (
    <div>
      <PageHeader
        eyebrow={
          <Link className="text-xs text-muted-foreground hover:text-foreground" to="/channels">
            ← 返回渠道列表
          </Link>
        }
        title={editing !== null ? '编辑渠道' : '新增渠道'}
      />
      {error ? <div className="mb-4"><ErrorAlert>{error}</ErrorAlert></div> : null}
      <form className="grid max-w-4xl gap-4" onSubmit={submit}>
        {editing === null ? (
          <Card className="shadow-none">
            <CardHeader>
              <CardTitle>预设</CardTitle>
            </CardHeader>
            <CardContent>
              <Field label="只填充表单，不会立刻保存">
                <Select
                  value={preset || undefined}
                  onValueChange={(id) => {
                    setPreset(id)
                    applyPreset(id)
                  }}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="选择原厂或三方渠道…" />
                  </SelectTrigger>
                  <SelectContent>
                    {groups.map((g) => (
                      <SelectGroup key={g}>
                        <SelectLabel>{g}</SelectLabel>
                        {CHANNEL_PRESETS.filter((p) => p.group === g).map((p) => (
                          <SelectItem key={p.id} value={p.id}>
                            {p.label}
                          </SelectItem>
                        ))}
                      </SelectGroup>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
            </CardContent>
          </Card>
        ) : null}

        <Card className="shadow-none">
          <CardHeader>
            <CardTitle>基础信息</CardTitle>
          </CardHeader>
          <CardContent className="grid gap-4 sm:grid-cols-2">
            <Field label="名称">
              <Input
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                required
              />
            </Field>
            <Field label="分组">
              <Input
                value={form.group_name}
                onChange={(e) => setForm({ ...form, group_name: e.target.value })}
              />
            </Field>
            <Field label="优先级">
              <Input
                type="number"
                value={form.priority}
                onChange={(e) => setForm({ ...form, priority: Number(e.target.value) })}
              />
            </Field>
            <Field label="权重" hint="同一优先级内按比例分配流量">
              <Input
                type="number"
                value={form.weight}
                onChange={(e) => setForm({ ...form, weight: Number(e.target.value) })}
              />
            </Field>
          </CardContent>
        </Card>

        <Card className="shadow-none">
          <CardHeader>
            <CardTitle>上游</CardTitle>
          </CardHeader>
          <CardContent className="grid gap-4">
            <Field label="Base URL">
              <Input
                value={form.base_url}
                onChange={(e) => setForm({ ...form, base_url: e.target.value })}
                required
              />
            </Field>
            <Field label={editing !== null ? '上游 API Key（留空不改）' : '上游 API Key'}>
              <Input
                value={form.key}
                onChange={(e) => setForm({ ...form, key: e.target.value })}
                required={editing === null}
              />
            </Field>
            <div className="grid gap-4 sm:grid-cols-3">
              <Field label="上游协议">
                <Select value={form.upstream_protocol} onValueChange={setProtocol}>
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {(meta?.protocols ?? Object.keys(PROTOCOL_LABELS)).map((p) => (
                      <SelectItem key={p} value={p}>
                        {PROTOCOL_LABELS[p] ?? p}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
              <Field label="转发模式">
                <Select
                  value={form.relay_mode}
                  onValueChange={(relay_mode) => setForm({ ...form, relay_mode })}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {(meta?.relay_modes ?? ['passthrough', 'convert'])
                      .filter((m) => canConvert || m === 'passthrough')
                      .map((m) => (
                        <SelectItem key={m} value={m}>
                          {MODE_LABELS[m] ?? m}
                        </SelectItem>
                      ))}
                  </SelectContent>
                </Select>
              </Field>
              <Field label="鉴权方式">
                <Select
                  value={form.auth_style}
                  onValueChange={(auth_style) => setForm({ ...form, auth_style })}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {(meta?.auth_styles ?? ['bearer', 'x_api_key', 'x_goog_api_key']).map((a) => (
                      <SelectItem key={a} value={a}>
                        {a}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
            </div>
          </CardContent>
        </Card>

        <Card className="shadow-none">
          <CardHeader>
            <CardTitle>模型映射</CardTitle>
          </CardHeader>
          <CardContent className="grid gap-3">
            <div className="overflow-hidden rounded-lg border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>模型名（客户端）</TableHead>
                    <TableHead>上游模型名（空 = 同名）</TableHead>
                    <TableHead>上游方言</TableHead>
                    <TableHead />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.length === 0 ? (
                    <TableRow>
                      <TableCell colSpan={4} className="text-muted-foreground">
                        还没有模型
                      </TableCell>
                    </TableRow>
                  ) : (
                    rows.map((r, i) => (
                      <TableRow key={r.model}>
                        <TableCell className="font-mono text-xs">{r.model}</TableCell>
                        <TableCell>
                          <Input
                            value={r.upstream_model}
                            placeholder={r.model}
                            onChange={(e) =>
                              setRows(rows.map((x, j) => (j === i ? { ...x, upstream_model: e.target.value } : x)))
                            }
                          />
                        </TableCell>
                        <TableCell>
                          <DialectSelect
                            meta={meta}
                            protocol={form.upstream_protocol}
                            value={r.upstream_dialect}
                            onChange={(v) =>
                              setRows(rows.map((x, j) => (j === i ? { ...x, upstream_dialect: v } : x)))
                            }
                          />
                        </TableCell>
                        <TableCell>
                          <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="text-destructive"
                            onClick={() => setRows(rows.filter((_, j) => j !== i))}
                          >
                            移除
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))
                  )}
                </TableBody>
              </Table>
            </div>
            <div className="flex gap-2">
              <Input
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
              <Button type="button" variant="secondary" onClick={addModels}>
                添加
              </Button>
            </div>
          </CardContent>
        </Card>

        <Card className="shadow-none">
          <CardHeader>
            <CardTitle>高级</CardTitle>
          </CardHeader>
          <CardContent className="grid gap-4">
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label="新模型默认方言">
                <DialectSelect
                  meta={meta}
                  protocol={form.upstream_protocol}
                  value={form.default_dialect}
                  onChange={(v) => setForm({ ...form, default_dialect: v })}
                />
              </Field>
              <Field label="原厂标识（vendor，聚合渠道留空）">
                <Input
                  value={form.vendor}
                  onChange={(e) => setForm({ ...form, vendor: e.target.value })}
                />
              </Field>
            </div>
            <label className="flex items-center gap-2 text-sm">
              <Checkbox
                checked={form.inject_stream_usage}
                onCheckedChange={(checked) =>
                  setForm({ ...form, inject_stream_usage: checked === true })
                }
              />
              流式请求补 include_usage（计费需要）
            </label>
            <p className="text-xs leading-relaxed text-muted-foreground">
              方言是上游对该模型实际遵循的厂商规范。客户端协议与该模型的原厂方言都和这一行一致时原样透传，否则经中间表示转换；opaque（聚合渠道）只透传同协议请求。转发模式只决定能否服务其他协议。
            </p>
          </CardContent>
        </Card>

        <div className="flex gap-2">
          <Button type="submit">保存</Button>
          <Button variant="outline" asChild>
            <Link to="/channels">取消</Link>
          </Button>
        </div>
      </form>
    </div>
  )
}
