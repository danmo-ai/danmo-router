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
  register: (username: string, password: string) =>
    request('/api/user/register', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    }),
  login: (username: string, password: string) =>
    request('/api/user/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    }),
  logout: () => request('/api/user/logout', { method: 'POST' }),
  me: () => request<any>('/api/user/me'),
  tokens: () => request<any[]>('/api/user/tokens'),
  createToken: (body: any) =>
    request<{ token: any; key: string }>('/api/user/tokens', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  setTokenStatus: (id: number, status: string) =>
    request(`/api/user/tokens/${id}/status`, {
      method: 'POST',
      body: JSON.stringify({ status }),
    }),
  deleteToken: (id: number) =>
    request(`/api/user/tokens/${id}`, { method: 'DELETE' }),
  logs: () => request<any[]>('/api/user/logs?limit=100'),
}
