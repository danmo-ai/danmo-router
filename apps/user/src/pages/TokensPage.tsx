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
import { RowActions } from '@/components/row-actions'
import { StatusBadge } from '@/components/status-badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import {
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

const EMPTY = {
  name: 'default',
  unlimited_quota: true,
  remain_quota: 0,
  expired_at: '',
  models: '',
  group_name: '',
}

export function TokensPage() {
  const [tokens, setTokens] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [form, setForm] = useState({ ...EMPTY })
  const [editing, setEditing] = useState<number | null>(null)
  const [open, setOpen] = useState(false)
  const [createdKey, setCreatedKey] = useState('')
  const [error, setError] = useState('')
  const { ask, dialog } = useConfirm()

  async function refresh() {
    setTokens(await api.tokens())
  }
  useEffect(() => {
    refresh()
      .catch(console.error)
      .finally(() => setLoading(false))
  }, [])

  function payload() {
    return {
      name: form.name,
      unlimited_quota: form.unlimited_quota,
      remain_quota: form.unlimited_quota ? -1 : form.remain_quota,
      expired_at: form.expired_at || null,
      models: form.models,
      group_name: form.group_name || null,
    }
  }

  function closeForm() {
    setOpen(false)
    setEditing(null)
    setForm({ ...EMPTY })
    setError('')
  }

  return (
    <div>
      <PageHeader
        title="API 令牌"
        description="完整密钥只在创建时显示一次。"
        actions={
          <Button
            type="button"
            onClick={() => {
              setEditing(null)
              setForm({ ...EMPTY })
              setError('')
              setOpen(true)
            }}
          >
            新增令牌
          </Button>
        }
      />
      <DataTable>
        <table className="w-full caption-bottom text-sm">
          <TableHeader>
            <TableRow>
              <TableHead>名称</TableHead>
              <TableHead>前缀</TableHead>
              <TableHead>状态</TableHead>
              <TableHead>额度</TableHead>
              <TableHead>模型</TableHead>
              <TableHead>分组</TableHead>
              <TableHead>过期</TableHead>
              <TableHead className="text-right">操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {loading ? (
              <SkeletonRows cols={8} />
            ) : tokens.length === 0 ? (
              <EmptyRow colSpan={8}>还没有令牌</EmptyRow>
            ) : (
              tokens.map((t) => (
                <TableRow key={t.id}>
                  <TableCell className="font-medium">{t.name}</TableCell>
                  <TableCell className="font-mono text-xs">{t.key_prefix}...</TableCell>
                  <TableCell>
                    <StatusBadge status={t.status} />
                  </TableCell>
                  <TableCell>{t.unlimited_quota ? '无限' : t.remain_quota}</TableCell>
                  <TableCell className="max-w-48 whitespace-normal">{t.models || '全部'}</TableCell>
                  <TableCell>{t.group_name || '账户'}</TableCell>
                  <TableCell className="font-mono text-xs">{t.expired_at || '-'}</TableCell>
                  <TableCell>
                    <RowActions
                      primary={
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => {
                            setEditing(t.id)
                            setError('')
                            setForm({
                              name: t.name,
                              unlimited_quota: Boolean(t.unlimited_quota),
                              remain_quota: t.unlimited_quota ? 0 : t.remain_quota,
                              expired_at: toLocalInput(t.expired_at),
                              models: t.models || '',
                              group_name: t.group_name || '',
                            })
                            setOpen(true)
                          }}
                        >
                          编辑
                        </Button>
                      }
                      menu={[
                        {
                          label: t.status === 'active' ? '禁用' : '启用',
                          destructive: t.status === 'active',
                          onSelect: () => {
                            if (t.status === 'active') {
                              ask({
                                title: `禁用令牌 ${t.name}？`,
                                description: '禁用后使用该令牌的请求会被拒绝。',
                                confirmLabel: '禁用',
                                destructive: true,
                                run: () => setStatus(t, 'disabled'),
                              })
                            } else {
                              void setStatus(t, 'active')
                            }
                          },
                        },
                        {
                          label: '删除',
                          destructive: true,
                          onSelect: () =>
                            ask({
                              title: `删除令牌 ${t.name}？`,
                              description: '删除后不可恢复，已发出的密钥立即失效。',
                              confirmLabel: '删除',
                              destructive: true,
                              run: () => remove(t.id),
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
      {open ? (
        <Modal title={editing !== null ? '编辑令牌' : '新增令牌'} onClose={closeForm}>
          {error ? <ErrorAlert>{error}</ErrorAlert> : null}
          <form
            className="grid gap-4"
            onSubmit={async (e) => {
              e.preventDefault()
              setError('')
              try {
                if (editing !== null) {
                  await api.updateToken(editing, payload())
                  toast.success('令牌已更新')
                } else {
                  const created = await api.createToken(payload())
                  setCreatedKey(created.key)
                }
                closeForm()
                await refresh()
              } catch (err: any) {
                setError(err.message)
              }
            }}
          >
            <Field label="令牌名称">
              <Input
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                required
              />
            </Field>
            <label className="flex items-center gap-2 text-sm">
              <Checkbox
                checked={form.unlimited_quota}
                onCheckedChange={(checked) =>
                  setForm({ ...form, unlimited_quota: checked === true })
                }
              />
              无限额度（仍受账户额度限制）
            </label>
            <Field label="令牌额度">
              <Input
                type="number"
                value={form.remain_quota}
                disabled={form.unlimited_quota}
                onChange={(e) => setForm({ ...form, remain_quota: Number(e.target.value) })}
              />
            </Field>
            <Field label="过期时间（可空）">
              <Input
                type="datetime-local"
                value={form.expired_at}
                onChange={(e) => setForm({ ...form, expired_at: e.target.value })}
              />
            </Field>
            <Field label="模型白名单（逗号分隔，空 = 全部）">
              <Input
                value={form.models}
                onChange={(e) => setForm({ ...form, models: e.target.value })}
              />
            </Field>
            <Field label="分组覆盖（空 = 用账户分组）">
              <Input
                value={form.group_name}
                onChange={(e) => setForm({ ...form, group_name: e.target.value })}
              />
            </Field>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="outline" onClick={closeForm}>
                取消
              </Button>
              <Button type="submit">保存</Button>
            </div>
          </form>
        </Modal>
      ) : null}
      {createdKey ? (
        <Modal title="新令牌" onClose={() => setCreatedKey('')}>
          <p className="text-sm text-muted-foreground">密钥只显示这一次，请立刻复制保存。</p>
          <code className="block rounded-lg border bg-muted/40 p-3 font-mono text-xs break-all">
            {createdKey}
          </code>
          <div className="flex justify-end">
            <CopyButton text={createdKey} label="复制密钥" />
          </div>
        </Modal>
      ) : null}
      {dialog}
    </div>
  )

  async function setStatus(t: any, status: string) {
    try {
      await api.setTokenStatus(t.id, status)
      await refresh()
    } catch (err: any) {
      toast.error(err.message)
      throw err
    }
  }

  async function remove(id: number) {
    try {
      await api.deleteToken(id)
      toast.success('令牌已删除')
      await refresh()
    } catch (err: any) {
      toast.error(err.message)
      throw err
    }
  }
}

function toLocalInput(value: string | null): string {
  if (!value) return ''
  const normalized = value.includes('T') ? value : value.replace(' ', 'T')
  return normalized.slice(0, 16)
}
