import { useEffect, useState } from 'react'
import { api } from '../api'

export function RedemptionsPage() {
  const [rows, setRows] = useState<any[]>([])
  const [name, setName] = useState('')
  const [quota, setQuota] = useState(100000)
  const [count, setCount] = useState(1)
  const [created, setCreated] = useState<any[]>([])
  const [error, setError] = useState('')
  const [copied, setCopied] = useState('')

  async function refresh() {
    setRows(await api.redemptions())
  }
  useEffect(() => {
    refresh().catch(console.error)
  }, [])

  return (
    <div>
      <h1>兑换码</h1>
      {error && <div className="error">{error}</div>}
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
      <form
        className="card form-grid"
        onSubmit={async (e) => {
          e.preventDefault()
          setError('')
          setCopied('')
          try {
            const made = await api.createRedemptions({ name, quota, count })
            setCreated(made)
            setName('')
            await refresh()
          } catch (err: any) {
            setError(err.message)
          }
        }}
      >
        <input
          placeholder="名称（可选）"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <input
          type="number"
          min={1}
          placeholder="额度"
          value={quota}
          onChange={(e) => setQuota(Number(e.target.value))}
          required
        />
        <input
          type="number"
          min={1}
          max={100}
          placeholder="数量"
          value={count}
          onChange={(e) => setCount(Number(e.target.value))}
          required
        />
        <button type="submit">生成</button>
      </form>
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
              <td>{r.status === 'unused' ? '未使用' : '已使用'}</td>
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
    </div>
  )
}
