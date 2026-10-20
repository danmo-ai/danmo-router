import { useEffect, useState } from 'react'
import { api } from '../api'

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
  const [form, setForm] = useState({ ...EMPTY })
  const [editing, setEditing] = useState<number | null>(null)
  const [createdKey, setCreatedKey] = useState('')
  const [error, setError] = useState('')

  async function refresh() {
    setTokens(await api.tokens())
  }
  useEffect(() => {
    refresh().catch(console.error)
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

  return (
    <div>
      <h1>API 令牌</h1>
      {error && <div className="error">{error}</div>}
      {createdKey && (
        <div className="card success">
          新令牌（仅显示一次）：<code>{createdKey}</code>
        </div>
      )}
      <form
        className="card form-grid"
        onSubmit={async (e) => {
          e.preventDefault()
          setError('')
          try {
            if (editing !== null) {
              await api.updateToken(editing, payload())
              setEditing(null)
            } else {
              const created = await api.createToken(payload())
              setCreatedKey(created.key)
            }
            setForm({ ...EMPTY })
            await refresh()
          } catch (err: any) {
            setError(err.message)
          }
        }}
      >
        <input
          value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })}
          placeholder="令牌名称"
          required
        />
        <label className="checkbox">
          <input
            type="checkbox"
            checked={form.unlimited_quota}
            onChange={(e) => setForm({ ...form, unlimited_quota: e.target.checked })}
          />
          无限额度（仍受账户额度限制）
        </label>
        <input
          type="number"
          placeholder="令牌额度"
          value={form.remain_quota}
          disabled={form.unlimited_quota}
          onChange={(e) => setForm({ ...form, remain_quota: Number(e.target.value) })}
        />
        <label>
          过期时间（可空）
          <input
            type="datetime-local"
            value={form.expired_at}
            onChange={(e) => setForm({ ...form, expired_at: e.target.value })}
          />
        </label>
        <input
          value={form.models}
          onChange={(e) => setForm({ ...form, models: e.target.value })}
          placeholder="模型白名单，逗号分隔，空 = 全部"
        />
        <input
          value={form.group_name}
          onChange={(e) => setForm({ ...form, group_name: e.target.value })}
          placeholder="分组覆盖（空 = 用账户分组）"
        />
        <div className="actions wide">
          <button type="submit">{editing !== null ? '保存令牌' : '创建令牌'}</button>
          {editing !== null && (
            <button
              type="button"
              className="ghost"
              onClick={() => {
                setEditing(null)
                setForm({ ...EMPTY })
              }}
            >
              取消
            </button>
          )}
        </div>
      </form>
      <table>
        <thead>
          <tr>
            <th>名称</th>
            <th>前缀</th>
            <th>状态</th>
            <th>额度</th>
            <th>模型</th>
            <th>分组</th>
            <th>过期</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {tokens.map((t) => (
            <tr key={t.id}>
              <td>{t.name}</td>
              <td>{t.key_prefix}...</td>
              <td>{t.status}</td>
              <td>{t.unlimited_quota ? '无限' : t.remain_quota}</td>
              <td>{t.models || '全部'}</td>
              <td>{t.group_name || '账户'}</td>
              <td>{t.expired_at || '-'}</td>
              <td className="actions">
                <button
                  onClick={() => {
                    setEditing(t.id)
                    setCreatedKey('')
                    setForm({
                      name: t.name,
                      unlimited_quota: Boolean(t.unlimited_quota),
                      remain_quota: t.unlimited_quota ? 0 : t.remain_quota,
                      expired_at: toLocalInput(t.expired_at),
                      models: t.models || '',
                      group_name: t.group_name || '',
                    })
                  }}
                >
                  编辑
                </button>
                <button
                  onClick={async () => {
                    await api.setTokenStatus(t.id, t.status === 'active' ? 'disabled' : 'active')
                    await refresh()
                  }}
                >
                  {t.status === 'active' ? '禁用' : '启用'}
                </button>
                <button
                  className="danger"
                  onClick={async () => {
                    await api.deleteToken(t.id)
                    await refresh()
                  }}
                >
                  删除
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function toLocalInput(value: string | null): string {
  if (!value) return ''
  const normalized = value.includes('T') ? value : value.replace(' ', 'T')
  return normalized.slice(0, 16)
}
