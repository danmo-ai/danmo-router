import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { Activity, AlertTriangle, Coins, Users } from 'lucide-react'
import { api } from '../api'
import { PageHeader } from '@/components/page-header'
import { StatCard } from '@/components/stat-card'
import { Skeleton } from '@/components/ui/skeleton'

export function OverviewPage() {
  const [stats, setStats] = useState<any>(null)
  useEffect(() => {
    api.overview().then(setStats).catch(console.error)
  }, [])
  if (!stats) {
    return (
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {Array.from({ length: 7 }, (_, i) => (
          <Skeleton key={i} className="h-24 rounded-xl" />
        ))}
      </div>
    )
  }
  const rate =
    stats.total_requests > 0
      ? ((stats.success_requests / stats.total_requests) * 100).toFixed(1)
      : '0.0'
  const rateNum = Number(rate)
  return (
    <div>
      <PageHeader title="概览" description="请求、额度与资源的当前快照。" />
      <Section title="流量">
        <StatCard label="总请求" value={stats.total_requests} icon={Activity} />
        <StatCard
          label="成功率"
          value={`${rate}%`}
          icon={Activity}
          tone={rateNum >= 99 ? 'good' : rateNum < 90 ? 'bad' : 'default'}
        />
        <StatCard label="今日请求" value={stats.today_requests} icon={Activity} />
        <StatCard
          label="失败数"
          value={stats.failed_requests}
          icon={AlertTriangle}
          tone={stats.failed_requests > 0 ? 'bad' : 'good'}
        />
      </Section>
      <Section title="额度">
        <StatCard label="消耗额度" value={stats.total_quota} icon={Coins} />
        <StatCard label="今日消耗" value={stats.today_quota} icon={Coins} />
      </Section>
      <Section title="资源">
        <StatCard
          label="用户 / 渠道"
          value={`${stats.total_users} / ${stats.total_channels}`}
          icon={Users}
        />
      </Section>
    </div>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mb-6">
      <h2 className="mb-2 text-xs font-medium text-muted-foreground">{title}</h2>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">{children}</div>
    </section>
  )
}
