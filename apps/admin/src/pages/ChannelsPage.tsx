import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { api } from '../api'
import { statusLabel } from '../shared'
import { MODE_LABELS, PROTOCOL_LABELS } from '../presets'

export function ChannelsPage() {
  const [channels, setChannels] = useState<any[]>([])
  const [probe, setProbe] = useState('')

  async function refresh() {
    setChannels(await api.channels())
  }
  useEffect(() => {
    refresh().catch(console.error)
  }, [])

  return (
    <div>
      <div className="page-header">
        <h1>渠道</h1>
        <Link className="button" to="/channels/new">
          新增渠道
        </Link>
      </div>
      {probe && <p className="hint">{probe}</p>}
      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>名称</th>
            <th>上游协议</th>
            <th>模式</th>
            <th>Base URL</th>
            <th>模型 · 方言</th>
            <th>分组</th>
            <th>优先级</th>
            <th>权重</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {channels.map((c) => (
            <tr key={c.id}>
              <td>{c.id}</td>
              <td>{c.name}</td>
              <td>{PROTOCOL_LABELS[c.upstream_protocol] ?? c.upstream_protocol}</td>
              <td>{MODE_LABELS[c.relay_mode] ?? c.relay_mode}</td>
              <td>{c.base_url}</td>
              <td className="mono">
                {(c.models ?? []).map((r: any) => (
                  <div key={r.id}>
                    {r.model}
                    {r.upstream_model ? ` → ${r.upstream_model}` : ''} · {r.upstream_dialect}
                  </div>
                ))}
              </td>
              <td>{c.group_name}</td>
              <td>{c.priority}</td>
              <td>{c.weight}</td>
              <td>{statusLabel(c.status)}</td>
              <td className="actions">
                <Link className="button subtle" to={`/channels/${c.id}/edit`}>
                  编辑
                </Link>
                <button
                  onClick={async () => {
                    setProbe(`正在测试渠道 ${c.id}…`)
                    try {
                      const r = await api.testChannel(c.id)
                      setProbe(
                        r.ok
                          ? `渠道 ${c.name} 连通 ${r.status}，${r.latency_ms}ms`
                          : `渠道 ${c.name} 失败 ${r.status || '网络错误'} ${r.error || ''}（${r.latency_ms}ms）`,
                      )
                    } catch (err: any) {
                      setProbe(err.message)
                    }
                  }}
                >
                  测试
                </button>
                <button
                  onClick={async () => {
                    await api.setChannelStatus(
                      c.id,
                      c.status === 'enabled' ? 'disabled' : 'enabled',
                    )
                    await refresh()
                  }}
                >
                  {c.status === 'enabled' ? '禁用' : '启用'}
                </button>
                <button
                  className="danger"
                  onClick={async () => {
                    await api.deleteChannel(c.id)
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
