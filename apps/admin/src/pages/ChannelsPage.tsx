import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { toast } from 'sonner'
import { api } from '../api'
import { MODE_LABELS, PROTOCOL_LABELS } from '../presets'
import { useConfirm } from '@/components/confirm-dialog'
import { DataTable, EmptyRow, SkeletonRows } from '@/components/data-table'
import { PageHeader } from '@/components/page-header'
import { RowActions } from '@/components/row-actions'
import { StatusBadge } from '@/components/status-badge'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

export function ChannelsPage() {
  const [channels, setChannels] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const { ask, dialog } = useConfirm()

  async function refresh() {
    setChannels(await api.channels())
  }
  useEffect(() => {
    refresh()
      .catch(console.error)
      .finally(() => setLoading(false))
  }, [])

  return (
    <div>
      <PageHeader
        title="渠道"
        description="上游连接、转发模式和模型映射。"
        actions={
          <Button asChild>
            <Link to="/channels/new">新增渠道</Link>
          </Button>
        }
      />
      <DataTable>
        <table className="w-full caption-bottom text-sm">
          <TableHeader>
            <TableRow>
              <TableHead>ID</TableHead>
              <TableHead>名称</TableHead>
              <TableHead>上游协议</TableHead>
              <TableHead>模式</TableHead>
              <TableHead>Base URL</TableHead>
              <TableHead>模型 · 方言</TableHead>
              <TableHead>分组</TableHead>
              <TableHead className="text-right">优先级</TableHead>
              <TableHead className="text-right">权重</TableHead>
              <TableHead>状态</TableHead>
              <TableHead className="text-right">操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {loading ? (
              <SkeletonRows cols={11} />
            ) : channels.length === 0 ? (
              <EmptyRow colSpan={11}>还没有渠道</EmptyRow>
            ) : (
              channels.map((c) => (
                <TableRow key={c.id}>
                  <TableCell className="tabular-nums text-muted-foreground">{c.id}</TableCell>
                  <TableCell className="font-medium">{c.name}</TableCell>
                  <TableCell>
                    <Badge variant="secondary">{PROTOCOL_LABELS[c.upstream_protocol] ?? c.upstream_protocol}</Badge>
                  </TableCell>
                  <TableCell>
                    <Badge variant="outline">{MODE_LABELS[c.relay_mode] ?? c.relay_mode}</Badge>
                  </TableCell>
                  <TableCell>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <span className="block max-w-[180px] truncate font-mono text-xs">{c.base_url}</span>
                      </TooltipTrigger>
                      <TooltipContent>{c.base_url}</TooltipContent>
                    </Tooltip>
                  </TableCell>
                  <TableCell className="whitespace-normal">
                    <ModelList models={c.models ?? []} />
                  </TableCell>
                  <TableCell>{c.group_name}</TableCell>
                  <TableCell className="text-right tabular-nums">{c.priority}</TableCell>
                  <TableCell className="text-right tabular-nums">{c.weight}</TableCell>
                  <TableCell>
                    <StatusBadge status={c.status} />
                  </TableCell>
                  <TableCell>
                    <RowActions
                      primary={
                        <Button variant="ghost" size="sm" asChild>
                          <Link to={`/channels/${c.id}/edit`}>编辑</Link>
                        </Button>
                      }
                      menu={[
                        {
                          label: '测试',
                          onSelect: () => void testChannel(c),
                        },
                        {
                          label: c.status === 'enabled' ? '禁用' : '启用',
                          destructive: c.status === 'enabled',
                          onSelect: () => {
                            if (c.status === 'enabled') {
                              ask({
                                title: `禁用渠道 ${c.name}？`,
                                description: '禁用后该渠道不再承接新请求。',
                                confirmLabel: '禁用',
                                destructive: true,
                                run: () => toggle(c),
                              })
                            } else {
                              void toggle(c)
                            }
                          },
                        },
                        {
                          label: '删除',
                          destructive: true,
                          onSelect: () =>
                            ask({
                              title: `删除渠道 ${c.name}？`,
                              description: '删除后不可恢复。',
                              confirmLabel: '删除',
                              destructive: true,
                              run: () => remove(c.id),
                            }),
                        },
                      ]}
                    />
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </table>
      </DataTable>
      {dialog}
    </div>
  )

  async function testChannel(c: any) {
    const id = toast.loading(`正在测试渠道 ${c.name}…`)
    try {
      const r = await api.testChannel(c.id)
      if (r.ok) {
        toast.success(`渠道 ${c.name} 连通 ${r.status}，${r.latency_ms}ms`, { id })
      } else {
        toast.error(
          `渠道 ${c.name} 失败 ${r.status || '网络错误'} ${r.error || ''}（${r.latency_ms}ms）`,
          { id },
        )
      }
    } catch (err: any) {
      toast.error(err.message, { id })
    }
  }

  async function toggle(c: any) {
    try {
      await api.setChannelStatus(c.id, c.status === 'enabled' ? 'disabled' : 'enabled')
      await refresh()
    } catch (err: any) {
      toast.error(err.message)
      throw err
    }
  }

  async function remove(id: number) {
    try {
      await api.deleteChannel(id)
      toast.success('渠道已删除')
      await refresh()
    } catch (err: any) {
      toast.error(err.message)
      throw err
    }
  }
}

function ModelList({ models }: { models: any[] }) {
  const [open, setOpen] = useState(false)
  const shown = open ? models : models.slice(0, 3)
  if (models.length === 0) return <span className="text-muted-foreground">-</span>
  return (
    <div className="space-y-0.5 font-mono text-xs whitespace-nowrap">
      {shown.map((r) => (
        <div key={r.id}>
          {r.model}
          {r.upstream_model ? ` → ${r.upstream_model}` : ''} · {r.upstream_dialect}
        </div>
      ))}
      {models.length > 3 ? (
        <button
          type="button"
          className="text-muted-foreground hover:text-foreground"
          onClick={() => setOpen((v) => !v)}
        >
          {open ? '收起' : `+${models.length - 3}`}
        </button>
      ) : null}
    </div>
  )
}
