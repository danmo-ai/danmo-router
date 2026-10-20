import { useEffect, useState } from 'react'
import type { FormEvent, ReactNode } from 'react'
import { NavLink, useNavigate } from 'react-router-dom'
import { api } from './api'
import { ComplianceAuthNotice, ComplianceSiteFooter } from './compliance'

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
    <div className="auth-page">
      <form className="card" onSubmit={submit}>
        <h1>用户中心</h1>
        <p className="muted">danmo-router User Portal</p>
        {error && <div className="error">{error}</div>}
        <label>
          用户名
          <input value={username} onChange={(e) => setUsername(e.target.value)} required />
        </label>
        <label>
          密码
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            minLength={6}
          />
        </label>
        {mode === 'register' && (
          <div className="captcha-row">
            <label className="captcha-answer">
              验证码
              <input
                value={captchaAnswer}
                onChange={(e) => setCaptchaAnswer(e.target.value)}
                required
                autoComplete="off"
                spellCheck={false}
                maxLength={8}
              />
            </label>
            <button
              type="button"
              className="captcha-image ghost"
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
                />
              ) : (
                <span className="muted">加载中</span>
              )}
            </button>
          </div>
        )}
        <button type="submit">{mode === 'login' ? '登录' : '注册'}</button>
        <button
          type="button"
          className="ghost"
          onClick={() => setMode(mode === 'login' ? 'register' : 'login')}
        >
          {mode === 'login' ? '没有账号？注册' : '已有账号？登录'}
        </button>
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
    <div className="layout">
      <header>
        <div className="brand">Danmo User</div>
        <nav>
          <NavLink to="/">概览</NavLink>
          <NavLink to="/tokens">令牌</NavLink>
          <NavLink to="/models">模型</NavLink>
          <NavLink to="/logs">日志</NavLink>
        </nav>
        <div className="header-right">
          <span>{user.username}</span>
          <button className="ghost" onClick={onLogout}>
            退出
          </button>
        </div>
      </header>
      <div className="layout-body">
        <main>{children}</main>
        <ComplianceSiteFooter />
      </div>
    </div>
  )
}

export const PAGE_SIZE = 50
