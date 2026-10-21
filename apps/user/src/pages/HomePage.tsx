import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { toast } from 'sonner'
import { api } from '../api'
import { CopyButton } from '@/components/copy-button'
import { ErrorAlert } from '@/components/error-alert'
import { Field } from '@/components/field'
import { PageHeader } from '@/components/page-header'
import { StatCard } from '@/components/stat-card'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { fmtNum } from '@/lib/format'

export function HomePage({
  user,
  onUser,
}: {
  user: any
  onUser: (u: any) => void
}) {
  const [profile, setProfile] = useState(user)
  const [relay, setRelay] = useState('http://127.0.0.1:3000/v1')
  const [code, setCode] = useState('')
  const [oldPassword, setOldPassword] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')

  useEffect(() => {
    api
      .me()
      .then((u) => {
        setProfile(u)
        onUser(u)
      })
      .catch(console.error)
    api
      .meta()
      .then((m) => setRelay(m.relay_base_url))
      .catch(console.error)
  }, [onUser])

  async function redeem(e: FormEvent) {
    e.preventDefault()
    setError('')
    try {
      const u = await api.redeem(code.trim())
      setProfile(u)
      onUser(u)
      setCode('')
      toast.success(`已兑换，剩余额度 ${fmtNum(u.remaining_quota)}`)
    } catch (err: any) {
      setError(err.message)
    }
  }

  async function changePassword(e: FormEvent) {
    e.preventDefault()
    setError('')
    try {
      await api.changePassword(oldPassword, password)
      setOldPassword('')
      setPassword('')
      toast.success('密码已更新')
    } catch (err: any) {
      setError(err.message)
    }
  }

  const total = Number(profile.quota) || 0
  const used = Number(profile.used_quota) || 0
  const remaining = profile.remaining_quota ?? profile.quota - profile.used_quota
  const pct = total > 0 ? Math.min(100, (used / total) * 100) : 0

  return (
    <div>
      <PageHeader title="账户概览" description={`分组 ${profile.group_name}`} />
      {error ? <div className="mb-4"><ErrorAlert>{error}</ErrorAlert></div> : null}
      <div className="mb-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard label="剩余额度" value={Number(remaining) || 0} />
        <StatCard label="总额度" value={total} />
        <StatCard label="已用额度" value={used} />
        <StatCard label="分组" value={String(profile.group_name ?? '')} />
      </div>
      <Card className="mb-4 shadow-none">
        <CardContent className="pt-6">
          <div className="mb-2 flex items-center justify-between text-xs text-muted-foreground">
            <span>额度使用</span>
            <span className="tabular-nums">{pct.toFixed(1)}%</span>
          </div>
          <div className="h-2 overflow-hidden rounded-full bg-muted">
            <div className="h-full rounded-full bg-primary" style={{ width: `${pct}%` }} />
          </div>
        </CardContent>
      </Card>
      <Card className="mb-4 shadow-none">
        <CardHeader>
          <CardTitle>调用方式</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-3">
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border bg-muted/40 px-3 py-2">
            <div className="min-w-0">
              <div className="text-xs text-muted-foreground">Base URL</div>
              <code className="font-mono text-xs break-all">{relay}</code>
            </div>
            <CopyButton text={relay} />
          </div>
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border bg-muted/40 px-3 py-2">
            <div className="min-w-0">
              <div className="text-xs text-muted-foreground">Header</div>
              <code className="font-mono text-xs">Authorization: Bearer sk-...</code>
            </div>
            <CopyButton text="Authorization: Bearer sk-..." />
          </div>
        </CardContent>
      </Card>
      <div className="grid gap-4 lg:grid-cols-2">
        <Card className="shadow-none">
          <CardHeader>
            <CardTitle>兑换额度</CardTitle>
          </CardHeader>
          <CardContent>
            <form className="flex gap-2" onSubmit={redeem}>
              <Input
                value={code}
                onChange={(e) => setCode(e.target.value)}
                placeholder="兑换码"
                required
              />
              <Button type="submit">兑换</Button>
            </form>
          </CardContent>
        </Card>
        <Card className="shadow-none">
          <CardHeader>
            <CardTitle>修改密码</CardTitle>
          </CardHeader>
          <CardContent>
            <form className="grid gap-3" onSubmit={changePassword}>
              <Field label="当前密码">
                <Input
                  type="password"
                  value={oldPassword}
                  onChange={(e) => setOldPassword(e.target.value)}
                  required
                />
              </Field>
              <Field label="新密码（至少 6 位）">
                <Input
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                  minLength={6}
                />
              </Field>
              <div>
                <Button type="submit">更新密码</Button>
              </div>
            </form>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
