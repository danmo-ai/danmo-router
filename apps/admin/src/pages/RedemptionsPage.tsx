import { useEffect, useState } from 'react'
import { toast } from 'sonner'
import { api } from '../api'
import { Modal } from '../shared'
import { useConfirm } from '@/components/confirm-dialog'
import { CopyButton } from '@/components/copy-button'
import { DataTable, EmptyRow, SkeletonRows } from '@/components/data-table'
import { ErrorAlert } from '@/components/error-alert'
import { Field } from '@/components/field'
import { PageHeader } from '@/components/page-header'
import { StatusBadge } from '@/components/status-badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { fmtNum } from '@/lib/format'

export function RedemptionsPage() {
  const [rows, setRows] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [name, setName] = useState('')
  const [quota, setQuota] = useState(100000)
  const [count, setCount] = useState(1)
  const [created, setCreated] = useState<any[]>([])
  const [error, setError] = useState('')
  const [creating, setCreating] = useState(false)
  const { ask, dialog } = useConfirm()

  async function refresh() {
    setRows(await api.redemptions())
  }
  useEffect(() => {
    refresh()
      .catch(console.error)
      .finally(() => setLoading(false))
  }, [])

  function closeCreate() {
    setCreating(false)
    setError('')
  }

  return (
    <div>
      <PageHeader
        title="兑换码"
        description="生成后可复制未使用的码。"
        actions={
          <Button type="button" onClick={() => setCreating(true)}>
            生成兑换码
          </Button>
        }
      />
      {error && !creating ? <div className="mb-4"><ErrorAlert>{error}</ErrorAlert></div> : null}
      <DataTable>
        <table className="w-full caption-bottom text-sm">
          <TableHeader>
            <TableRow>
              <TableHead>ID</TableHead>
              <TableHead>名称</TableHead>
              <TableHead>兑换码</TableHead>
              <TableHead className="text-right">额度</TableHead>
              <TableHead>状态</TableHead>
              <TableHead>使用者</TableHead>
              <TableHead>时间</TableHead>
              <TableHead className="text-right" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {loading ? (
              <SkeletonRows cols={8} />
            ) : rows.length === 0 ? (
              <EmptyRow colSpan={8}>还没有兑换码</EmptyRow>
            ) : (
              rows.map((r) => (
                <TableRow key={r.id}>
                  <TableCell className="tabular-nums text-muted-foreground">{r.id}</TableCell>
                  <TableCell>{r.name || '-'}</TableCell>
                  <TableCell className="font-mono text-xs">{r.code}</TableCell>
                  <TableCell className="text-right tabular-nums">{fmtNum(r.quota)}</TableCell>
                  <TableCell>
                    <StatusBadge status={r.status} />
                  </TableCell>
                  <TableCell>{r.redeemed_by ?? '-'}</TableCell>
                  <TableCell className="font-mono text-xs">{r.redeemed_at || r.created_at}</TableCell>
                  <TableCell className="text-right">
                    {r.status === 'unused' ? (
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="text-destructive"
                        onClick={() =>
                          ask({
                            title: '删除兑换码？',
                            description: `${r.code} 删除后不可恢复。`,
                            confirmLabel: '删除',
                            destructive: true,
                            run: async () => {
                              setError('')
                              try {
                                await api.deleteRedemption(r.id)
                                toast.success('兑换码已删除')
                                await refresh()
                              } catch (err: any) {
                                setError(err.message)
                                throw err
                              }
                            },
                          })
                        }
                      >
                        删除
                      </Button>
                    ) : null}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </table>
      </DataTable>
      {creating ? (
        <Modal title="生成兑换码" onClose={closeCreate}>
          {error ? <ErrorAlert>{error}</ErrorAlert> : null}
          <form
            className="grid gap-4"
            onSubmit={async (e) => {
              e.preventDefault()
              setError('')
              try {
                const made = await api.createRedemptions({ name, quota, count })
                setCreated(made)
                setName('')
                closeCreate()
                await refresh()
              } catch (err: any) {
                setError(err.message)
              }
            }}
          >
            <Field label="名称（可选）">
              <Input value={name} onChange={(e) => setName(e.target.value)} />
            </Field>
            <Field label="额度">
              <Input
                type="number"
                min={1}
                value={quota}
                onChange={(e) => setQuota(Number(e.target.value))}
                required
              />
            </Field>
            <Field label="数量">
              <Input
                type="number"
                min={1}
                max={100}
                value={count}
                onChange={(e) => setCount(Number(e.target.value))}
                required
              />
            </Field>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="outline" onClick={closeCreate}>
                取消
              </Button>
              <Button type="submit">生成</Button>
            </div>
          </form>
        </Modal>
      ) : null}
      {created.length > 0 ? (
        <Modal title="新兑换码" onClose={() => setCreated([])}>
          <p className="text-sm text-muted-foreground">已写入列表。关闭后仍可在表格里复制未使用的码。</p>
          <div className="grid gap-1 rounded-lg border bg-muted/40 p-3 font-mono text-xs">
            {created.map((c) => (
              <div key={c.id}>{c.code}</div>
            ))}
          </div>
          <div className="flex justify-end">
            <CopyButton text={created.map((c) => c.code).join('\n')} label="复制本次生成" />
          </div>
        </Modal>
      ) : null}
      {dialog}
    </div>
  )
}
