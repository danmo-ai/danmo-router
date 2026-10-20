import { useEffect, useState } from 'react'
import { api } from '../api'
import { PAGE_SIZE } from '../shared'

export function LogsPage() {
  const [logs, setLogs] = useState<any[]>([])
  const [userId, setUserId] = useState('')
  const [model, setModel] = useState('')
  const [channelId, setChannelId] = useState('')
  const [status, setStatus] = useState('')
  const [offset, setOffset] = useState(0)
  const [error, setError] = useState('')

  async function load(nextOffset: number) {
    setError('')
    try {
      const rows = await api.logs({
        limit: PAGE_SIZE,
        offset: nextOffset,
        user_id: userId.trim(),
        model: model.trim(),
        status,
        channel_id: channelId.trim(),
      })
      setLogs(rows)
      setOffset(nextOffset)
    } catch (err: any) {
      setError(err.message)
    }
  }

  useEffect(() => {
    load(0).catch(console.error)
    // Initial page only; filters apply from the form.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <div>
      <h1>调用日志</h1>
      {error && <div className="error">{error}</div>}
      <form
        className="filter-bar"
        onSubmit={(e) => {
          e.preventDefault()
          load(0).catch(console.error)
        }}
      >
        <input
          placeholder="用户 ID"
          value={userId}
          onChange={(e) => setUserId(e.target.value)}
        />
        <input placeholder="模型" value={model} onChange={(e) => setModel(e.target.value)} />
        <input
          placeholder="渠道 ID"
          value={channelId}
          onChange={(e) => setChannelId(e.target.value)}
        />
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
            <th>用户</th>
            <th>渠道</th>
            <th>模型</th>
            <th>输入</th>
            <th>缓存读 / 写</th>
            <th>输出（推理）</th>
            <th>用量格式</th>
            <th>额度</th>
            <th>耗时</th>
            <th>状态</th>
          </tr>
        </thead>
        <tbody>
          {logs.map((l) => (
            <tr key={l.id}>
              <td>{l.created_at}</td>
              <td>{l.user_id}</td>
              <td>{l.channel_id ?? '-'}</td>
              <td>{l.model}</td>
              <td>{l.prompt_tokens}</td>
              <td>
                {l.cache_read_tokens} / {l.cache_write_tokens}
              </td>
              <td>
                {l.completion_tokens}
                {l.reasoning_tokens > 0 ? `（${l.reasoning_tokens}）` : ''}
              </td>
              <td className="mono">{l.usage_adapter || '-'}</td>
              <td>{l.quota}</td>
              <td>{l.duration_ms}ms</td>
              <td className={`status-${l.status}`} title={l.error ?? l.raw_usage ?? ''}>
                {l.status === 'billing_failed' ? '计费失败' : l.status}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="pager">
        <button type="button" disabled={offset === 0} onClick={() => load(Math.max(0, offset - PAGE_SIZE))}>
          上一页
        </button>
        <span>第 {offset / PAGE_SIZE + 1} 页</span>
        <button type="button" disabled={logs.length < PAGE_SIZE} onClick={() => load(offset + PAGE_SIZE)}>
          下一页
        </button>
      </div>
    </div>
  )
}
