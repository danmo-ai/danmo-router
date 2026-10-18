export type ApiResponse<T> = {
  success: boolean
  message?: string
  data?: T
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers || {}),
    },
    ...init,
  })
  const json = (await res.json()) as ApiResponse<T>
  if (!res.ok || !json.success) {
    throw new Error(json.message || `请求失败 (${res.status})`)
  }
  return json.data as T
}

export const api = {
  login: (username: string, password: string) =>
    request('/api/admin/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    }),
  logout: () => request('/api/admin/logout', { method: 'POST' }),
  me: () => request<any>('/api/admin/me'),
  overview: () => request<any>('/api/admin/overview'),
  channels: () => request<any[]>('/api/admin/channels'),
  createChannel: (body: any) =>
    request('/api/admin/channels', { method: 'POST', body: JSON.stringify(body) }),
  updateChannel: (id: number, body: any) =>
    request(`/api/admin/channels/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
  setChannelStatus: (id: number, status: string) =>
    request(`/api/admin/channels/${id}/status`, {
      method: 'PUT',
      body: JSON.stringify({ status }),
    }),
  deleteChannel: (id: number) =>
    request(`/api/admin/channels/${id}`, { method: 'DELETE' }),
  users: () => request<any[]>('/api/admin/users'),
  createUser: (body: any) =>
    request('/api/admin/users', { method: 'POST', body: JSON.stringify(body) }),
  updateUser: (id: number, body: any) =>
    request(`/api/admin/users/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
  adjustQuota: (id: number, delta: number) =>
    request(`/api/admin/users/${id}/quota`, {
      method: 'POST',
      body: JSON.stringify({ delta }),
    }),
  modelPrices: () => request<any[]>('/api/admin/model-prices'),
  upsertModelPrice: (body: any) =>
    request('/api/admin/model-prices', { method: 'PUT', body: JSON.stringify(body) }),
  logs: () => request<any[]>('/api/admin/logs?limit=100'),
}
