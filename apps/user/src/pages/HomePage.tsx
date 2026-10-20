import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { api } from '../api'

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
  const [message, setMessage] = useState('')
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
    setMessage('')
    try {
      const u = await api.redeem(code.trim())
      setProfile(u)
      onUser(u)
      setCode('')
      setMessage(`已兑换，剩余额度 ${u.remaining_quota}`)
    } catch (err: any) {
      setError(err.message)
    }
  }

  async function changePassword(e: FormEvent) {
    e.preventDefault()
    setError('')
    setMessage('')
    try {
      await api.changePassword(oldPassword, password)
      setOldPassword('')
      setPassword('')
      setMessage('密码已更新')
    } catch (err: any) {
      setError(err.message)
    }
  }

  return (
    <div>
      <h1>账户概览</h1>
      {error && <div className="error">{error}</div>}
      {message && <div className="card success">{message}</div>}
      <div className="grid">
        <div className="stat">
          <div className="label">剩余额度</div>
          <div className="value">{profile.remaining_quota ?? profile.quota - profile.used_quota}</div>
        </div>
        <div className="stat">
          <div className="label">总额度</div>
          <div className="value">{profile.quota}</div>
        </div>
        <div className="stat">
          <div className="label">已用额度</div>
          <div className="value">{profile.used_quota}</div>
        </div>
        <div className="stat">
          <div className="label">分组</div>
          <div className="value">{profile.group_name}</div>
        </div>
      </div>
      <div className="card tip">
        <h3>调用方式</h3>
        <p>
          Base URL: <code>{relay}</code>
        </p>
        <p>
          Header: <code>Authorization: Bearer sk-...</code>
        </p>
      </div>
      <form className="card form-row" onSubmit={redeem}>
        <input
          value={code}
          onChange={(e) => setCode(e.target.value)}
          placeholder="兑换码"
          required
        />
        <button type="submit">兑换额度</button>
      </form>
      <form className="card form-grid" onSubmit={changePassword}>
        <h3>修改密码</h3>
        <input
          type="password"
          placeholder="当前密码"
          value={oldPassword}
          onChange={(e) => setOldPassword(e.target.value)}
          required
        />
        <input
          type="password"
          placeholder="新密码（至少 6 位）"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          required
          minLength={6}
        />
        <button type="submit">更新密码</button>
      </form>
    </div>
  )
}
