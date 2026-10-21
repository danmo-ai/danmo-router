import { Badge } from '@/components/ui/badge'
import { statusLabel } from '@/shared'

const GOOD = new Set(['success', 'enabled', 'active', 'unused'])
const BAD = new Set(['error', 'billing_failed'])
const MUTED = new Set(['disabled', 'used'])

export function StatusBadge({ status }: { status: string }) {
  const tone = GOOD.has(status)
    ? 'border-emerald-200 bg-emerald-50 text-emerald-700'
    : BAD.has(status)
      ? 'border-red-200 bg-red-50 text-red-700'
      : MUTED.has(status)
        ? 'border-transparent bg-muted text-muted-foreground'
        : ''
  return (
    <Badge variant="outline" className={tone}>
      {statusLabel(status)}
    </Badge>
  )
}
