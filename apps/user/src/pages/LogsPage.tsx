import { useEffect, useState } from 'react'
import { api } from '../api'
import { PAGE_SIZE } from '../shared'

export function LogsPage() {
  const [logs, setLogs] = useState<any[]>([])
  const [model, setModel] = useState('')
  const [status, setStatus] = useState('')
  const [offset, setOffset] = useState(0)
  const [error, setError] = useState('')

  async function load(nextOffset: number) {
    setError('')
    const rows = await api.logs({
      limit: PAGE_SIZE,
      offset: nextOffset,
      model: model.trim(),
      status,
    })
    setLogs(rows)
    setOffset(nextOffset)
  }

  useEffect(() => {
    load(0).catch((err: any) => setError(err.message))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <div>
      <h1>我的调用日志</h1>
      {error && <div className="error">{error}</div>}
      <form
        className="filter-bar"
        onSubmit={(e) => {
          e.preventDefault()
          load(0).catch((err: any) => setError(err.message))
        }}
      >
        <input placeholder="模型" value={model} onChange={(e) => setModel(e.target.value)} />
        <select value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">全部状态</option>
          <option value="success">success</option>
          <option value="error">error</option>
          <option value="billing_failed">billing_failed</option>
        </select>
        <button type="submit">筛选</button>
      </form>
      <table>
        <thead>
          <tr>
            <th>时间</th>
            <th>模型</th>
            <th>Prompt</th>
            <th>Completion</th>
            <th>额度</th>
            <th>状态</th>
          </tr>
        </thead>
        <tbody>
          {logs.map((l) => (
            <tr key={l.id}>
              <td>{l.created_at}</td>
              <td>{l.model}</td>
              <td>{l.prompt_tokens}</td>
              <td>{l.completion_tokens}</td>
              <td>{l.quota}</td>
              <td>{l.status}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="pager">
        <button type="button" disabled={offset === 0} onClick={() => load(Math.max(0, offset - PAGE_SIZE)).catch((err: any) => setError(err.message))}>
          上一页
        </button>
        <span>第 {offset / PAGE_SIZE + 1} 页</span>
        <button
          type="button"
          disabled={logs.length < PAGE_SIZE}
          onClick={() => load(offset + PAGE_SIZE).catch((err: any) => setError(err.message))}
        >
          下一页
        </button>
      </div>
    </div>
  )
}
