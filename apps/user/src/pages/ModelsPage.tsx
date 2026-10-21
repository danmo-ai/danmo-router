import { useEffect, useState } from 'react'
import { api } from '../api'
import { DataTable, EmptyRow, SkeletonRows } from '@/components/data-table'
import { ErrorAlert } from '@/components/error-alert'
import { PageHeader } from '@/components/page-header'
import {
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

export function ModelsPage() {
  const [catalog, setCatalog] = useState<any>(null)
  const [error, setError] = useState('')

  useEffect(() => {
    api
      .models()
      .then(setCatalog)
      .catch((err: any) => setError(err.message))
  }, [])

  if (error) return <ErrorAlert>{error}</ErrorAlert>

  const models = catalog?.models ?? []

  return (
    <div>
      <PageHeader
        title="可用模型"
        description={
          catalog
            ? `分组 ${catalog.group_name}，分组倍率 ${catalog.group_ratio}。有效倍率 = 模型输入倍率 × 分组倍率。未定价的模型仍可路由。`
            : '正在加载当前分组可路由的模型。'
        }
      />
      <DataTable>
        <table className="w-full caption-bottom text-sm">
          <TableHeader>
            <TableRow>
              <TableHead>模型</TableHead>
              <TableHead className="text-right">输入倍率</TableHead>
              <TableHead className="text-right">输出 / 输入</TableHead>
              <TableHead className="text-right">有效倍率</TableHead>
              <TableHead>已配置协议</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {!catalog ? (
              <SkeletonRows cols={5} />
            ) : models.length === 0 ? (
              <EmptyRow colSpan={5}>当前分组还没有可路由的模型。请管理员在渠道里添加模型。</EmptyRow>
            ) : (
              models.map((m: any) => (
                <TableRow key={m.model}>
                  <TableCell className="font-medium">{m.model}</TableCell>
                  <TableCell className="text-right tabular-nums">{m.model_ratio ?? '未定价'}</TableCell>
                  <TableCell className="text-right tabular-nums">{m.completion_ratio ?? '-'}</TableCell>
                  <TableCell className="text-right tabular-nums">{m.effective_ratio ?? '-'}</TableCell>
                  <TableCell className="whitespace-normal">{(m.protocols ?? []).join(', ') || '-'}</TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </table>
      </DataTable>
    </div>
  )
}
