import { useEffect, useState } from 'react'
import type { FormEvent, ReactNode } from 'react'
import { KeyRound, LayoutDashboard, ScrollText, Shapes } from 'lucide-react'
import { NavLink, useNavigate } from 'react-router-dom'
import { api } from './api'
import { ComplianceAuthNotice, ComplianceSiteFooter } from './compliance'
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
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { cn } from 'cn'

const NAV = [
  { to: '/', label: '概览', icon: LayoutDashboard, end: true },
  { to: '/tokens', label: '令牌', icon: KeyRound, end: false },
  { to: '/models', label: '模型', icon: Shapes, end: false },
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
  return { user, setUser, refresh: () => api.me().then(setUser) }
}

export function AuthPage({ onLogin }: { onLogin: (u: any) => void }) {
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [captchaToken, setCaptchaToken] = useState('')
  const [captchaSvg, setCaptchaSvg] = useState('')
  const [captchaAnswer, setCaptchaAnswer] = useState('')
  const [error, setError] = useState('')
  const navigate = useNavigate()

  async function loadCaptcha() {
    try {
      const c = await api.captcha()
      setCaptchaToken(c.token)
      setCaptchaSvg(c.image_svg)
      setCaptchaAnswer('')
    } catch (err: any) {
      setError(err.message || '验证码加载失败')
    }
  }

  useEffect(() => {
    if (mode === 'register') {
      void loadCaptcha()
    } else {
      setCaptchaToken('')
      setCaptchaSvg('')
      setCaptchaAnswer('')
    }
  }, [mode])

  async function submit(e: FormEvent) {
    e.preventDefault()
    setError('')
    try {
      const u =
        mode === 'login'
          ? await api.login(username, password)
          : await api.register(username, password, captchaToken, captchaAnswer)
      onLogin(u)
      navigate('/')
    } catch (err: any) {
      setError(err.message)
      if (mode === 'register') {
        void loadCaptcha()
      }
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
            <h1 className="text-lg font-semibold tracking-tight">用户中心</h1>
            <p className="text-xs text-muted-foreground">danmo-router User Portal</p>
          </div>
        </div>
        <Tabs value={mode} onValueChange={(value) => setMode(value as 'login' | 'register')}>
          <TabsList className="grid w-full grid-cols-2">
            <TabsTrigger value="login">登录</TabsTrigger>
            <TabsTrigger value="register">注册</TabsTrigger>
          </TabsList>
        </Tabs>
        {error ? <ErrorAlert>{error}</ErrorAlert> : null}
        <Field label="用户名">
          <Input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            required
            autoComplete="username"
          />
        </Field>
        <Field label="密码">
          <Input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            minLength={6}
            autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
          />
        </Field>
        {mode === 'register' ? (
          <div className="flex items-end gap-2">
            <Field label="验证码" className="min-w-0 flex-1">
              <Input
                value={captchaAnswer}
                onChange={(e) => setCaptchaAnswer(e.target.value)}
                required
                autoComplete="off"
                spellCheck={false}
                maxLength={8}
              />
            </Field>
            <button
              type="button"
              className="grid h-9 w-40 shrink-0 place-items-center overflow-hidden rounded-md border bg-muted"
              onClick={() => void loadCaptcha()}
              title="点击刷新验证码"
              aria-label="刷新验证码"
            >
              {captchaSvg ? (
                <img
                  src={`data:image/svg+xml;utf8,${encodeURIComponent(captchaSvg)}`}
                  alt="验证码"
                  width={160}
                  height={48}
                  className="h-9 w-40 object-cover"
                />
              ) : (
                <span className="text-xs text-muted-foreground">加载中</span>
              )}
            </button>
          </div>
        ) : null}
        <Button type="submit">{mode === 'login' ? '登录' : '注册'}</Button>
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
    <div className="flex min-h-svh flex-col">
      <header className="sticky top-0 z-30 border-b bg-background/95 backdrop-blur">
        <div className="mx-auto flex h-14 w-full max-w-6xl items-center gap-4 px-4 md:px-6">
          <div className="flex items-center gap-2">
            <div className="grid size-7 place-items-center rounded-md bg-primary text-xs font-semibold text-primary-foreground">
              D
            </div>
            <div className="text-sm font-semibold tracking-tight">Danmo</div>
          </div>
          <nav className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
            {NAV.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                end={item.end}
                className={({ isActive }) =>
                  cn(
                    'flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-sm text-muted-foreground whitespace-nowrap hover:bg-accent hover:text-foreground',
                    isActive && 'bg-accent font-medium text-foreground',
                  )
                }
              >
                <item.icon className="size-4" />
                {item.label}
              </NavLink>
            ))}
          </nav>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm" className="gap-2">
                <span className="grid size-6 place-items-center rounded-full bg-muted text-xs font-medium">
                  {String(user.username ?? '?').slice(0, 1).toUpperCase()}
                </span>
                <span className="max-w-28 truncate">{user.username}</span>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuLabel>{user.username}</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={onLogout}>退出</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </header>
      <div className="flex flex-1 flex-col">
        <main className="mx-auto w-full max-w-6xl flex-1 p-4 md:p-6">{children}</main>
        <ComplianceSiteFooter />
      </div>
    </div>
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
    default:
      return status
  }
}
