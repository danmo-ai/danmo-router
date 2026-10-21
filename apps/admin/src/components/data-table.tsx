import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableCell, TableRow } from '@/components/ui/table'

export function DataTable({ children }: { children: ReactNode }) {
  return (
    <div className="overflow-hidden rounded-xl border bg-card">
      <div className="overflow-x-auto">{children}</div>
    </div>
  )
}

export function EmptyRow({ colSpan, children }: { colSpan: number; children: ReactNode }) {
  return (
    <TableRow className="hover:bg-transparent">
      <TableCell colSpan={colSpan} className="h-24 text-center whitespace-normal text-muted-foreground">
        {children}
      </TableCell>
    </TableRow>
  )
}

export function SkeletonRows({ rows = 4, cols }: { rows?: number; cols: number }) {
  return (
    <>
      {Array.from({ length: rows }, (_, i) => (
        <TableRow key={i} className="hover:bg-transparent">
          {Array.from({ length: cols }, (_, j) => (
            <TableCell key={j}>
              <Skeleton className="h-4 w-full max-w-24" />
            </TableCell>
          ))}
        </TableRow>
      ))}
    </>
  )
}

export function Pager({
  page,
  hasPrev,
  hasNext,
  onPrev,
  onNext,
}: {
  page: number
  hasPrev: boolean
  hasNext: boolean
  onPrev: () => void
  onNext: () => void
}) {
  return (
    <div className="flex items-center justify-end gap-3 pt-3 text-sm text-muted-foreground">
      <Button type="button" variant="outline" size="sm" disabled={!hasPrev} onClick={onPrev}>
        上一页
      </Button>
      <span className="tabular-nums">第 {page} 页</span>
      <Button type="button" variant="outline" size="sm" disabled={!hasNext} onClick={onNext}>
        下一页
      </Button>
    </div>
  )
}

export { Table }
