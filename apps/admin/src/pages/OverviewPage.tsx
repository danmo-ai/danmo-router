import { useEffect, useState } from 'react'
import { api } from '../api'

export function OverviewPage() {
  const [stats, setStats] = useState<any>(null)
  useEffect(() => {
    api.overview().then(setStats).catch(console.error)
  }, [])
  if (!stats) return <p>加载中...</p>
  const rate =
    stats.total_requests > 0
      ? ((stats.success_requests / stats.total_requests) * 100).toFixed(1)
      : '0.0'
  return (
    <div>
      <h1>概览</h1>
      <div className="grid">
        <Stat label="总请求" value={stats.total_requests} />
        <Stat label="成功率" value={`${rate}%`} />
        <Stat label="消耗额度" value={stats.total_quota} />
        <Stat label="用户 / 渠道" value={`${stats.total_users} / ${stats.total_channels}`} />
        <Stat label="今日请求" value={stats.today_requests} />
        <Stat label="失败数" value={stats.failed_requests} />
        <Stat label="今日消耗" value={stats.today_quota} />
      </div>
    </div>
  )
}

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="stat">
      <div className="label">{label}</div>
      <div className="value">{value}</div>
    </div>
  )
}
