import { useEffect, useState } from 'react'
import { api } from '../api'
import { Modal, statusLabel } from '../shared'

export function RedemptionsPage() {
  const [rows, setRows] = useState<any[]>([])
  const [name, setName] = useState('')
  const [quota, setQuota] = useState(100000)
  const [count, setCount] = useState(1)
  const [created, setCreated] = useState<any[]>([])
  const [error, setError] = useState('')
  const [copied, setCopied] = useState('')
  const [creating, setCreating] = useState(false)

  async function refresh() {
    setRows(await api.redemptions())
  }
  useEffect(() => {
    refresh().catch(console.error)
  }, [])

  function closeCreate() {
    setCreating(false)
    setError('')
  }

  return (
    <div>
      <div className="page-header">
        <h1>兑换码</h1>
        <button type="button" onClick={() => setCreating(true)}>
          生成兑换码
        </button>
      </div>
      {error && !creating && <div className="error">{error}</div>}
      {created.length > 0 && (
        <div className="card success">
          <div>新兑换码（已写入列表，可随时复制未使用的码）：</div>
          {created.map((c) => (
            <code key={c.id}>{c.code}</code>
          ))}
          <button
            type="button"
            onClick={async () => {
              try {
                await navigator.clipboard.writeText(created.map((c) => c.code).join('\n'))
                setCopied('已复制')
              } catch (err: any) {
                setCopied(err.message || '复制失败')
              }
            }}
          >
            复制本次生成
          </button>
          {copied && <span className="muted">{copied}</span>}
        </div>
      )}
      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>名称</th>
            <th>兑换码</th>
            <th>额度</th>
            <th>状态</th>
            <th>使用者</th>
            <th>时间</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.id}>
              <td>{r.id}</td>
              <td>{r.name || '-'}</td>
              <td className="mono">{r.code}</td>
              <td>{r.quota}</td>
              <td>{statusLabel(r.status)}</td>
              <td>{r.redeemed_by ?? '-'}</td>
              <td>{r.redeemed_at || r.created_at}</td>
              <td>
                {r.status === 'unused' && (
                  <button
                    className="danger"
                    onClick={async () => {
                      setError('')
                      try {
                        await api.deleteRedemption(r.id)
                        await refresh()
                      } catch (err: any) {
                        setError(err.message)
                      }
                    }}
                  >
                    删除
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {creating && (
        <Modal title="生成兑换码" onClose={closeCreate}>
          {error && <div className="error">{error}</div>}
          <form
            className="form-stack"
            onSubmit={async (e) => {
              e.preventDefault()
              setError('')
              setCopied('')
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
            <label>
              名称（可选）
              <input value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            <label>
              额度
              <input
                type="number"
                min={1}
                value={quota}
                onChange={(e) => setQuota(Number(e.target.value))}
                required
              />
            </label>
            <label>
              数量
              <input
                type="number"
                min={1}
                max={100}
                value={count}
                onChange={(e) => setCount(Number(e.target.value))}
                required
              />
            </label>
            <div className="form-actions">
              <button type="submit">生成</button>
              <button type="button" className="ghost" onClick={closeCreate}>
                取消
              </button>
            </div>
          </form>
        </Modal>
      )}
    </div>
  )
}
