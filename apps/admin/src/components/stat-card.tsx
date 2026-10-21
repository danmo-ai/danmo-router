import type { LucideIcon } from 'lucide-react'
import { cn } from 'cn'
import { Card, CardContent } from '@/components/ui/card'
import { fmtNum } from '@/lib/format'

export function StatCard({
  label,
  value,
  hint,
  icon: Icon,
  tone = 'default',
}: {
  label: string
  value: string | number
  hint?: string
  icon?: LucideIcon
  tone?: 'default' | 'good' | 'bad'
}) {
  const text = typeof value === 'number' ? fmtNum(value) : value
  return (
    <Card className="gap-0 py-4 shadow-none">
      <CardContent className="px-4">
        <div className="flex items-center justify-between gap-2">
          <div className="text-xs text-muted-foreground">{label}</div>
          {Icon ? <Icon className="size-4 text-muted-foreground" /> : null}
        </div>
        <div
          className={cn(
            'mt-2 text-2xl font-semibold tracking-tight tabular-nums',
            tone === 'good' && 'text-emerald-600',
            tone === 'bad' && 'text-destructive',
          )}
        >
          {text}
        </div>
        {hint ? <div className="mt-1 text-xs text-muted-foreground">{hint}</div> : null}
      </CardContent>
    </Card>
  )
}
