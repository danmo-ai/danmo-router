import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { toast } from 'sonner'
import { api } from '../api'
import { Modal, roleLabel } from '../shared'
import { useConfirm } from '@/components/confirm-dialog'
import { DataTable, EmptyRow, SkeletonRows } from '@/components/data-table'
import { ErrorAlert } from '@/components/error-alert'
import { Field } from '@/components/field'
import { PageHeader } from '@/components/page-header'
import { RowActions } from '@/components/row-actions'
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
import { fmtNum } from '@/lib/format'

type Draft = {
  quota: number
  group_name: string
  role: string
  status: string
}

export function UsersPage() {
  const [users, setUsers] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [form, setForm] = useState({
    username: '',
    password: '',
    quota: 500000,
    group_name: 'default',
  })
  const [editing, setEditing] = useState<any | null>(null)
  const [draft, setDraft] = useState<Draft>({
    quota: 0,
    group_name: 'default',
    role: 'user',
    status: 'active',
  })
  const [openId, setOpenId] = useState<number | null>(null)
  const [tokens, setTokens] = useState<any[]>([])
  const [creating, setCreating] = useState(false)
  const [resetting, setResetting] = useState<any | null>(null)
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const { ask, dialog } = useConfirm()

  async function refresh() {
    setUsers(await api.users())
  }
  useEffect(() => {
    refresh()
      .catch(console.error)
      .finally(() => setLoading(false))
  }, [])

  async function loadTokens(userId: number) {
    setTokens(await api.userTokens(userId))
    setOpenId(userId)
  }

  function closeCreate() {
    setCreating(false)
    setError('')
  }

  return (
    <div>
      <PageHeader
        title="用户"
        description="额度、角色和令牌。"
        actions={
          <Button type="button" onClick={() => setCreating(true)}>
            新增用户
          </Button>
        }
      />
      {error && !creating && !editing && !resetting ? <div className="mb-4"><ErrorAlert>{error}</ErrorAlert></div> : null}
      <DataTable>
        <table className="w-full caption-bottom text-sm">
          <TableHeader>
            <TableRow>
              <TableHead>ID</TableHead>
              <TableHead>用户名</TableHead>
              <TableHead>角色</TableHead>
              <TableHead>额度</TableHead>
              <TableHead>已用</TableHead>
              <TableHead>分组</TableHead>
              <TableHead>状态</TableHead>
              <TableHead className="text-right">操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {loading ? (
              <SkeletonRows cols={8} />
            ) : users.length === 0 ? (
              <EmptyRow colSpan={8}>还没有用户</EmptyRow>
            ) : (
              users.map((u) => (
                <TableRow key={u.id}>
                  <TableCell className="tabular-nums text-muted-foreground">{u.id}</TableCell>
                  <TableCell className="font-medium">{u.username}</TableCell>
                  <TableCell>{roleLabel(u.role)}</TableCell>
                  <TableCell className="tabular-nums">{fmtNum(u.quota)}</TableCell>
                  <TableCell className="tabular-nums">{fmtNum(u.used_quota)}</TableCell>
                  <TableCell>{u.group_name}</TableCell>
                  <TableCell>
                    <StatusBadge status={u.status} />
                  </TableCell>
                  <TableCell>
                    <RowActions
                      primary={
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => {
                            setError('')
                            setEditing(u)
                            setDraft({
                              quota: u.quota,
                              group_name: u.group_name,
                              role: u.role,
                              status: u.status,
                            })
                          }}
                        >
                          编辑
                        </Button>
                      }
                      menu={[
                        {
                          label: '+10万额度',
                          onSelect: () => void adjust(u.id),
                        },
                        {
                          label: '重置密码',
                          onSelect: () => {
                            setError('')
                            setPassword('')
                            setResetting(u)
                          },
                        },
                        {
                          label: '令牌',
                          onSelect: () => void openTokens(u.id),
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

      {creating ? (
        <Modal title="新增用户" onClose={closeCreate}>
          {error ? <ErrorAlert>{error}</ErrorAlert> : null}
          <form
            className="grid gap-4"
            onSubmit={async (e) => {
              e.preventDefault()
              setError('')
              try {
                await api.createUser(form)
                setForm({ ...form, username: '', password: '' })
                closeCreate()
                toast.success('用户已创建')
                await refresh()
              } catch (err: any) {
                setError(err.message)
              }
            }}
          >
            <Field label="用户名">
              <Input
                value={form.username}
                onChange={(e) => setForm({ ...form, username: e.target.value })}
                required
              />
            </Field>
            <Field label="密码">
              <Input
                type="password"
                value={form.password}
                onChange={(e) => setForm({ ...form, password: e.target.value })}
                required
              />
            </Field>
            <Field label="额度">
              <Input
                type="number"
                value={form.quota}
                onChange={(e) => setForm({ ...form, quota: Number(e.target.value) })}
              />
            </Field>
            <Field label="分组">
              <Input
                value={form.group_name}
                onChange={(e) => setForm({ ...form, group_name: e.target.value })}
              />
            </Field>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="outline" onClick={closeCreate}>
                取消
              </Button>
              <Button type="submit">保存</Button>
            </div>
          </form>
        </Modal>
      ) : null}

      {editing ? (
        <Modal title={`编辑用户 · ${editing.username}`} onClose={() => setEditing(null)}>
          {error ? <ErrorAlert>{error}</ErrorAlert> : null}
          <form className="grid gap-4" onSubmit={(e) => void saveUser(e)}>
            <Field label="角色">
              <Select value={draft.role} onValueChange={(role) => setDraft({ ...draft, role })}>
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="user">用户</SelectItem>
                  <SelectItem value="admin">管理员</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            <Field label="额度">
              <Input
                type="number"
                value={draft.quota}
                onChange={(e) => setDraft({ ...draft, quota: Number(e.target.value) })}
              />
            </Field>
            <Field label="分组">
              <Input
                value={draft.group_name}
                onChange={(e) => setDraft({ ...draft, group_name: e.target.value })}
              />
            </Field>
            <Field label="状态">
              <Select value={draft.status} onValueChange={(status) => setDraft({ ...draft, status })}>
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="active">启用</SelectItem>
                  <SelectItem value="disabled">禁用</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="outline" onClick={() => setEditing(null)}>
                取消
              </Button>
              <Button type="submit">保存</Button>
            </div>
          </form>
        </Modal>
      ) : null}

      {resetting ? (
        <Modal title={`重置密码 · ${resetting.username}`} onClose={() => setResetting(null)}>
          {error ? <ErrorAlert>{error}</ErrorAlert> : null}
          <form className="grid gap-4" onSubmit={(e) => void resetPassword(e)}>
            <Field label="新密码（至少 6 位）">
              <Input
                type="password"
                value={password}
                minLength={6}
                required
                onChange={(e) => setPassword(e.target.value)}
              />
            </Field>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="outline" onClick={() => setResetting(null)}>
                取消
              </Button>
              <Button type="submit">更新密码</Button>
            </div>
          </form>
        </Modal>
      ) : null}

      {openId !== null ? (
        <Modal title={`用户 ${openId} 的令牌`} onClose={() => setOpenId(null)} className="sm:max-w-3xl">
          <div className="overflow-hidden rounded-lg border">
            <table className="w-full text-sm">
              <TableHeader>
                <TableRow>
                  <TableHead>名称</TableHead>
                  <TableHead>前缀</TableHead>
                  <TableHead>状态</TableHead>
                  <TableHead>额度</TableHead>
                  <TableHead>模型</TableHead>
                  <TableHead className="text-right">操作</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {tokens.length === 0 ? (
                  <EmptyRow colSpan={6}>没有令牌</EmptyRow>
                ) : (
                  tokens.map((t) => (
                    <TableRow key={t.id}>
                      <TableCell>{t.name}</TableCell>
                      <TableCell className="font-mono text-xs">{t.key_prefix}...</TableCell>
                      <TableCell>
                        <StatusBadge status={t.status} />
                      </TableCell>
                      <TableCell>{t.unlimited_quota ? '无限' : fmtNum(t.remain_quota)}</TableCell>
                      <TableCell className="whitespace-normal">{t.models || '全部'}</TableCell>
                      <TableCell>
                        <RowActions
                          menu={[
                            {
                              label: t.status === 'active' ? '禁用' : '启用',
                              destructive: t.status === 'active',
                              onSelect: () => {
                                if (t.status !== 'active') {
                                  void toggleToken(t)
                                  return
                                }
                                ask({
                                  title: `禁用令牌 ${t.name}？`,
                                  description: '禁用后使用该令牌的请求会被拒绝。',
                                  confirmLabel: '禁用',
                                  destructive: true,
                                  run: () => toggleToken(t),
                                })
                              },
                            },
                            {
                              label: '删除',
                              destructive: true,
                              onSelect: () =>
                                ask({
                                  title: `删除令牌 ${t.name}？`,
                                  description: '删除后不可恢复。',
                                  confirmLabel: '删除',
                                  destructive: true,
                                  run: () => removeToken(t.id),
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
          </div>
        </Modal>
      ) : null}
      {dialog}
    </div>
  )

  async function saveUser(e: FormEvent) {
    e.preventDefault()
    if (!editing) return
    setError('')
    try {
      await api.updateUser(editing.id, draft)
      setEditing(null)
      toast.success('用户已更新')
      await refresh()
    } catch (err: any) {
      setError(err.message)
    }
  }

  async function resetPassword(e: FormEvent) {
    e.preventDefault()
    if (!resetting) return
    setError('')
    try {
      await api.resetPassword(resetting.id, password)
      setResetting(null)
      toast.success('密码已重置')
    } catch (err: any) {
      setError(err.message)
    }
  }

  async function adjust(id: number) {
    setError('')
    try {
      await api.adjustQuota(id, 100000)
      toast.success('已增加 10 万额度')
      await refresh()
    } catch (err: any) {
      setError(err.message)
      toast.error(err.message)
    }
  }

  async function openTokens(id: number) {
    setError('')
    try {
      await loadTokens(id)
    } catch (err: any) {
      setError(err.message)
      toast.error(err.message)
    }
  }

  async function toggleToken(t: any) {
    if (openId === null) return
    await api.setUserTokenStatus(openId, t.id, t.status === 'active' ? 'disabled' : 'active')
    await loadTokens(openId)
  }

  async function removeToken(id: number) {
    if (openId === null) return
    await api.deleteUserToken(openId, id)
    await loadTokens(openId)
  }
}
