import { useEffect, useState } from 'react'
import { api } from '../api'

export function ModelsPage() {
  const [catalog, setCatalog] = useState<any>(null)
  const [error, setError] = useState('')

  useEffect(() => {
    api
      .models()
      .then(setCatalog)
      .catch((err: any) => setError(err.message))
  }, [])

  if (error) return <div className="error">{error}</div>
  if (!catalog) return <p>加载中...</p>

  return (
    <div>
      <h1>可用模型</h1>
      <p className="muted">
        分组 {catalog.group_name}，分组倍率 {catalog.group_ratio}。有效倍率 = 模型输入倍率 × 分组倍率。未定价的模型仍可路由。
      </p>
      <table>
        <thead>
          <tr>
            <th>模型</th>
            <th>输入倍率</th>
            <th>输出 / 输入</th>
            <th>有效倍率</th>
            <th>已配置协议</th>
          </tr>
        </thead>
        <tbody>
          {(catalog.models ?? []).map((m: any) => (
            <tr key={m.model}>
              <td>{m.model}</td>
              <td>{m.model_ratio ?? '未定价'}</td>
              <td>{m.completion_ratio ?? '-'}</td>
              <td>{m.effective_ratio ?? '-'}</td>
              <td>{(m.protocols ?? []).join(', ') || '-'}</td>
            </tr>
          ))}
          {(catalog.models ?? []).length === 0 && (
            <tr>
              <td colSpan={5}>当前分组还没有可路由的模型。请管理员在渠道里添加模型。</td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  )
}
