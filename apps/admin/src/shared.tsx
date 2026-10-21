import { useEffect, useState } from 'react'
import type { FormEvent, ReactNode } from 'react'
import {
  Cable,
  KeyRound,
  LayoutDashboard,
  ScrollText,
  Tags,
  Users,
} from 'lucide-react'
import { NavLink, useNavigate } from 'react-router-dom'
import { api } from './api'
import type { Dialect, Meta } from './api'
import { ComplianceAuthNotice, ComplianceFooterLink } from './compliance'
import { ErrorAlert } from '@/components/error-alert'
import { Field } from '@/components/field'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { cn } from 'cn'

const NAV = [
  { to: '/', label: '概览', icon: LayoutDashboard, end: true },
  { to: '/channels', label: '渠道', icon: Cable, end: false },
  { to: '/users', label: '用户', icon: Users, end: false },
  { to: '/prices', label: '价格', icon: Tags, end: false },
  { to: '/redemptions', label: '兑换码', icon: KeyRound, end: false },
  { to: '/logs', label: '日志', icon: ScrollText, end: false },
]

export function Modal({
  title,
  onClose,
  children,
  className,
}: {
  title: string
  onClose: () => void
  children: ReactNode
  className?: string
}) {
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className={cn('sm:max-w-lg', className)}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        {children}
      </DialogContent>
    </Dialog>
  )
}

export function useAuth() {
  const [user, setUser] = useState<any | null | undefined>(undefined)
  useEffect(() => {
    api.me().then(setUser).catch(() => setUser(null))
  }, [])
  return { user, setUser }
}

export function LoginPage({ onLogin }: { onLogin: (u: any) => void }) {
  const [username, setUsername] = useState('admin')
  const [password, setPassword] = useState('admin123')
  const [error, setError] = useState('')
  const navigate = useNavigate()

  async function submit(e: FormEvent) {
    e.preventDefault()
    setError('')
    try {
      const u = await api.login(username, password)
      onLogin(u)
      navigate('/')
    } catch (err: any) {
      setError(err.message)
    }
  }

  return (
    <div className="grid min-h-svh place-items-center bg-muted/40 p-6">
      <form
        className="grid w-full max-w-sm gap-4 rounded-xl border bg-card p-6 shadow-sm"
        onSubmit={submit}
      >
        <div className="flex items-center gap-3">
          <div className="grid size-9 place-items-center rounded-lg bg-primary text-sm font-semibold text-primary-foreground">
            D
          </div>
          <div>
            <h1 className="text-lg font-semibold tracking-tight">管理后台</h1>
            <p className="text-xs text-muted-foreground">danmo-router Admin Portal</p>
          </div>
        </div>
        {error ? <ErrorAlert>{error}</ErrorAlert> : null}
        <Field label="用户名">
          <Input value={username} onChange={(e) => setUsername(e.target.value)} autoComplete="username" />
        </Field>
        <Field label="密码">
          <Input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="current-password"
          />
        </Field>
        <Button type="submit">登录</Button>
        <ComplianceAuthNotice />
      </form>
    </div>
  )
}

export function Layout({
  user,
  onLogout,
  children,
}: {
  user: any
  onLogout: () => void
  children: ReactNode
}) {
  return (
    <div className="flex min-h-svh flex-col md:flex-row">
      <aside className="flex shrink-0 flex-col border-b bg-sidebar md:sticky md:top-0 md:h-svh md:w-60 md:border-r md:border-b-0">
        <div className="flex items-center gap-2 px-4 py-4">
          <div className="grid size-7 place-items-center rounded-md bg-primary text-xs font-semibold text-primary-foreground">
            D
          </div>
          <div className="text-sm font-semibold tracking-tight">Danmo Admin</div>
        </div>
        <nav className="flex gap-1 overflow-x-auto px-2 pb-2 md:flex-1 md:flex-col md:overflow-visible md:px-2">
          {NAV.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              className={({ isActive }) =>
                cn(
                  'flex items-center gap-2 rounded-md px-2.5 py-2 text-sm text-muted-foreground whitespace-nowrap hover:bg-sidebar-accent hover:text-sidebar-accent-foreground',
                  isActive && 'bg-sidebar-accent font-medium text-sidebar-accent-foreground',
                )
              }
            >
              <item.icon className="size-4" />
              {item.label}
            </NavLink>
          ))}
        </nav>
        <div className="hidden border-t p-2 md:block">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" className="w-full justify-start gap-2 px-2">
                <span className="grid size-7 place-items-center rounded-full bg-muted text-xs font-medium">
                  {String(user.username ?? '?').slice(0, 1).toUpperCase()}
                </span>
                <span className="truncate">{user.username}</span>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-52">
              <DropdownMenuLabel>{user.username}</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <div className="px-2 py-1.5">
                <ComplianceFooterLink />
              </div>
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={onLogout}>退出</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center justify-end border-b px-4 py-2 md:hidden">
          <Button variant="ghost" size="sm" onClick={onLogout}>
            退出 {user.username}
          </Button>
        </div>
        <main className="mx-auto w-full max-w-7xl flex-1 p-4 md:p-6">{children}</main>
      </div>
    </div>
  )
}

export function useMeta() {
  const [meta, setMeta] = useState<Meta | null>(null)
  useEffect(() => {
    api.meta().then(setMeta).catch(console.error)
  }, [])
  return meta
}

export function dialectsOf(meta: Meta | null, protocol: string): Dialect[] {
  const all = meta?.dialects.filter((d) => d.protocol === protocol) ?? []
  return all.length > 0 ? all : [{ protocol, name: 'standard', label: '', opaque: false, usage: [] }]
}

export function DialectSelect({
  meta,
  protocol,
  value,
  onChange,
}: {
  meta: Meta | null
  protocol: string
  value: string
  onChange: (v: string) => void
}) {
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {dialectsOf(meta, protocol).map((d) => (
          <SelectItem key={d.name} value={d.name} title={d.label}>
            {d.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

export const PAGE_SIZE = 50

export function statusLabel(status: string) {
  switch (status) {
    case 'success':
      return '成功'
    case 'error':
      return '错误'
    case 'billing_failed':
      return '计费失败'
    case 'enabled':
    case 'active':
      return '启用'
    case 'disabled':
      return '禁用'
    case 'unused':
      return '未使用'
    case 'used':
      return '已使用'
    default:
      return status
  }
}

export function roleLabel(role: string) {
  switch (role) {
    case 'admin':
      return '管理员'
    case 'user':
      return '用户'
    default:
      return role
  }
}
