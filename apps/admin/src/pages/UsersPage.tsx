import { useEffect, useState } from 'react'
import { api } from '../api'

type Draft = {
  quota: number
  group_name: string
  role: string
  status: string
}

export function UsersPage() {
  const [users, setUsers] = useState<any[]>([])
  const [form, setForm] = useState({
    username: '',
    password: '',
    quota: 500000,
    group_name: 'default',
  })
  const [editing, setEditing] = useState<number | null>(null)
  const [draft, setDraft] = useState<Draft>({
    quota: 0,
    group_name: 'default',
    role: 'user',
    status: 'active',
  })
  const [openId, setOpenId] = useState<number | null>(null)
  const [tokens, setTokens] = useState<any[]>([])
  const [error, setError] = useState('')

  async function refresh() {
    setUsers(await api.users())
  }
  useEffect(() => {
    refresh().catch(console.error)
  }, [])

  async function loadTokens(userId: number) {
    setTokens(await api.userTokens(userId))
    setOpenId(userId)
  }

  return (
    <div>
      <h1>用户</h1>
      {error && <div className="error">{error}</div>}
      <form
        className="card form-grid"
        onSubmit={async (e) => {
          e.preventDefault()
          setError('')
          try {
            await api.createUser(form)
            setForm({ ...form, username: '', password: '' })
            await refresh()
          } catch (err: any) {
            setError(err.message)
          }
        }}
      >
        <input
          placeholder="用户名"
          value={form.username}
          onChange={(e) => setForm({ ...form, username: e.target.value })}
          required
        />
        <input
          placeholder="密码"
          type="password"
          value={form.password}
          onChange={(e) => setForm({ ...form, password: e.target.value })}
          required
        />
        <input
          type="number"
          placeholder="额度"
          value={form.quota}
          onChange={(e) => setForm({ ...form, quota: Number(e.target.value) })}
        />
        <input
          placeholder="分组"
          value={form.group_name}
          onChange={(e) => setForm({ ...form, group_name: e.target.value })}
        />
        <button type="submit">创建用户</button>
      </form>
      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>用户名</th>
            <th>角色</th>
            <th>额度</th>
            <th>已用</th>
            <th>分组</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {users.map((u) => (
            <tr key={u.id}>
              <td>{u.id}</td>
              <td>{u.username}</td>
              <td>
                {editing === u.id ? (
                  <select
                    value={draft.role}
                    onChange={(e) => setDraft({ ...draft, role: e.target.value })}
                  >
                    <option value="user">user</option>
                    <option value="admin">admin</option>
                  </select>
                ) : (
                  u.role
                )}
              </td>
              <td>
                {editing === u.id ? (
                  <input
                    type="number"
                    value={draft.quota}
                    onChange={(e) => setDraft({ ...draft, quota: Number(e.target.value) })}
                  />
                ) : (
                  u.quota
                )}
              </td>
              <td>{u.used_quota}</td>
              <td>
                {editing === u.id ? (
                  <input
                    value={draft.group_name}
                    onChange={(e) => setDraft({ ...draft, group_name: e.target.value })}
                  />
                ) : (
                  u.group_name
                )}
              </td>
              <td>
                {editing === u.id ? (
                  <select
                    value={draft.status}
                    onChange={(e) => setDraft({ ...draft, status: e.target.value })}
                  >
                    <option value="active">active</option>
                    <option value="disabled">disabled</option>
                  </select>
                ) : (
                  u.status
                )}
              </td>
              <td className="actions">
                {editing === u.id ? (
                  <>
                    <button
                      onClick={async () => {
                        setError('')
                        try {
                          await api.updateUser(u.id, draft)
                          setEditing(null)
                          await refresh()
                        } catch (err: any) {
                          setError(err.message)
                        }
                      }}
                    >
                      保存
                    </button>
                    <button onClick={() => setEditing(null)}>取消</button>
                  </>
                ) : (
                  <button
                    onClick={() => {
                      setEditing(u.id)
                      setDraft({
                        quota: u.quota,
                        group_name: u.group_name,
                        role: u.role,
                        status: u.status,
                      })
                    }}
                  >
                    编辑
                  </button>
                )}
                <button
                  onClick={async () => {
                    setError('')
                    try {
                      await api.adjustQuota(u.id, 100000)
                      await refresh()
                    } catch (err: any) {
                      setError(err.message)
                    }
                  }}
                >
                  +10万
                </button>
                <button
                  onClick={async () => {
                    const password = window.prompt('新密码（至少 6 位）')
                    if (!password) return
                    setError('')
                    try {
                      await api.resetPassword(u.id, password)
                    } catch (err: any) {
                      setError(err.message)
                    }
                  }}
                >
                  重置密码
                </button>
                <button
                  onClick={async () => {
                    setError('')
                    try {
                      if (openId === u.id) {
                        setOpenId(null)
                        return
                      }
                      await loadTokens(u.id)
                    } catch (err: any) {
                      setError(err.message)
                    }
                  }}
                >
                  令牌
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {openId !== null && (
        <div className="card sub">
          <h2>用户 {openId} 的令牌</h2>
          <table>
            <thead>
              <tr>
                <th>名称</th>
                <th>前缀</th>
                <th>状态</th>
                <th>额度</th>
                <th>模型</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {tokens.map((t) => (
                <tr key={t.id}>
                  <td>{t.name}</td>
                  <td className="mono">{t.key_prefix}...</td>
                  <td>{t.status}</td>
                  <td>{t.unlimited_quota ? '无限' : t.remain_quota}</td>
                  <td>{t.models || '全部'}</td>
                  <td className="actions">
                    <button
                      onClick={async () => {
                        await api.setUserTokenStatus(
                          openId,
                          t.id,
                          t.status === 'active' ? 'disabled' : 'active',
                        )
                        await loadTokens(openId)
                      }}
                    >
                      {t.status === 'active' ? '禁用' : '启用'}
                    </button>
                    <button
                      className="danger"
                      onClick={async () => {
                        await api.deleteUserToken(openId, t.id)
                        await loadTokens(openId)
                      }}
                    >
                      删除
                    </button>
                  </td>
                </tr>
              ))}
              {tokens.length === 0 && (
                <tr>
                  <td colSpan={6}>没有令牌</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
