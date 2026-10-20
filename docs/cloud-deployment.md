# 云平台部署指南

本文说明在公有云/私有云上部署 danmo-router 的推荐拓扑：**转发面用四层（L4）负载均衡**、**后端 HTTP 健康检查**、**Portal 静态站点与 `/api` 同源反代**。通用运维（环境变量、升级、备份）见 [operations.md](operations.md)。

## 目标拓扑

```text
                    ┌─────────────────────────────────────────┐
  客户端 / SDK      │  转发面（公网、高并发、长连接）           │
       │            │  L4 LB (TCP) → danmo-relay × N :3000    │
       └──────────►│  （可选：节点上 Nginx 终结 TLS）          │
                    └─────────────────────────────────────────┘

  管理 Portal       admin.example.com
       │            ├─ CDN / 对象存储：静态 assets
       └──────────►└─ 同源 /api/* → danmo-admin :3001（内网）

  用户 Portal       user.example.com
       │            ├─ CDN / 对象存储：静态 assets
       └──────────►└─ 同源 /api/* → danmo-user :3002（内网）

  转发 API（可选独立域名）  api.example.com → 同一 L4 池 → relay :3000

  共享数据层        DATABASE_URL（SQLite 文件卷 / 未来 Postgres）
                    ENCRYPTION_KEY 全进程一致
```

| 入口 | 建议 LB 类型 | 原因 |
|------|--------------|------|
| `danmo-relay` | **四层 TCP/UDP LB** | 流式 SSE、长连接；避免七层对 body 缓冲或短 idle 超时 |
| `danmo-admin` / `danmo-user` | 七层 HTTPS 或内网直连 | 短请求、会话 Cookie；QPS 低 |
| Portal 静态 | CDN + 对象存储 | 缓存 `assets/*`；HTML 短缓存 |

## 为什么转发用四层 LB

- Relay 存在 **分钟级** 的流式响应；七层网关若开启响应缓冲、较小 `read timeout` 或未透传 `Connection`，容易断流。
- 四层只做 **连接分发**，HTTP/TLS 可在 **后端 Nginx/Caddy** 或 **客户端 → NLB 透传 → 后端 TLS** 处理，行为更可预期。
- 多副本时，单条 TCP 连接固定在一个后端（连接级亲和）；无 sticky 时每次新连接随机分配，符合无状态 relay 设计。

## 转发面：四层 LB 配置要点

### 监听器

| 项 | 推荐值 |
|----|--------|
| 前端协议 | `TCP 443`（或 `TCP 80` 仅内网调试） |
| 后端协议 | `TCP` 转发到实例 **3000**，或先到实例 **443**（由节点 Nginx 再转 127.0.0.1:3000） |
| 均衡算法 | 轮询 / 加权轮询；连接数高时可加权 |
| 空闲超时 | **≥ 3600s**（与流式读超时同量级；按云厂商上限尽量调大） |
| 跨可用区 | 开启，避免单 AZ 故障 |

### TLS 两种常见模式

**模式 A — NLB/TCP 透传 + 节点 Nginx 终结 TLS（推荐）**

```text
Client ──TLS──► L4:443 ──TCP──► 节点 Nginx:443 ──HTTP──► danmo-relay:3000
```

- 证书用 Let’s Encrypt / 云证书服务，部署在每个节点或 Sidecar Nginx。
- Nginx 必须：`proxy_buffering off`、长 `proxy_read_timeout`（见 [operations.md — Nginx relay](operations.md#生产部署拆分)）。

**模式 B — 直连 relay 端口（内网或测试）**

```text
Client ──► L4:3000 ──TCP──► danmo-relay:3000
```

- 无 TLS，仅适合 VPC 内或配合 Service Mesh；公网务必套 TLS。

### 后端实例（relay）

- 进程监听 `HOST=0.0.0.0`、`PORT=3000`（默认）。
- 安全组 / 防火墙：**仅允许 L4 健康探测源与 LB 子网** 访问 3000（或 Nginx 443），勿对公网裸奔 admin/user 端口。
- 水平扩展：增加后端池成员；注意 SQLite 写放大时换网络库（见 [operations.md — 扩缩容](operations.md#扩缩容与容量)）。

## 后端健康检查

所有二进制均提供：

```http
GET /healthz
```

响应：`200`，body 纯文本 `ok`（无 JSON）。**不要用** `/v1/models` 探活（需 API Key）。

### 转发池（relay）— 推荐 HTTP 探活

即使监听器是 **L4 TCP**，多数云厂商仍允许对 **后端 IP:端口** 做 HTTP 检查（检查在 LB → 实例之间完成，不改变 L4 转发语义）。

| 参数 | 推荐值 |
|------|--------|
| 检查协议 | HTTP |
| 端口 | `3000`（若 TLS 在 Nginx 且仅暴露 443，可对 Nginx 配置 `location /healthz` 转 3000，或单独开 3000 仅给 LB 探活） |
| 路径 | `/healthz` |
| 成功码 | `200` |
| 间隔 | 5–10s |
| 超时 | 2–5s |
| 健康阈值 | 连续 2–3 次成功 |
| 不健康阈值 | 连续 2–3 次失败 |

**TCP 探活（备选）**：仅 `TCP connect :3000`，只能证明端口打开，**无法**发现进程僵死；生产优先 HTTP `/healthz`。

### 管理 / 用户 API 池

| 服务 | 端口 | 健康路径 |
|------|------|----------|
| `danmo-admin` | 3001 | `/healthz` |
| `danmo-user` | 3002 | `/healthz` |

Admin API 建议 **不对公网开放**，由 Portal 所在七层网关或内网 LB 回源；若必须公网，加 IP 白名单 / WAF。

### 滚动发布与健康

1. 新实例启动并通过 `/healthz` 后再注册到目标组。
2. 摘流：目标组标记 unhealthy 或权重置 0 → 等待已有长连接结束（relay 可配合较长 `deregistration delay`）。
3. 先升级 admin/user，再滚动 relay（与 [operations.md — 升级](operations.md#升级与数据库迁移) 一致）。

## Portal 部署

Portal 为 Vite 构建的 **SPA**，生产构建产物在 `apps/admin/dist`、`apps/user/dist`（或 `make package` → `dist/apps/*`）。

### 构建

```bash
# 管理 Portal
cd apps/admin
npm ci
VITE_COMPLIANCE_URL=https://your-docs.example.com/compliance.html npm run build

# 用户 Portal
cd apps/user
npm ci
VITE_COMPLIANCE_URL=https://your-docs.example.com/compliance.html npm run build
```

- 未设置 `VITE_COMPLIANCE_URL` 时，页脚链到仓库默认 [compliance.md](compliance.md) 的 GitHub 地址。
- 前端请求使用 **相对路径** `/api/...` 且 `credentials: 'include'`（Session Cookie）。**推荐** Portal 域名与 API **同源**，由网关把 `/api` 反代到 admin/user，而不是浏览器跨域访问 `admin-api.example.com`（跨站 Cookie 在 modern 浏览器下易失败）。

### 部署方式 A — 对象存储 + CDN（推荐）

1. 将 `dist/` 上传到桶（S3 / OSS / COS / GCS 等），开启静态网站或 CDN 源站。
2. **CDN 或源站 Nginx 配置回源规则**：
   - `/assets/*` → 静态文件，长缓存。
   - `/api/*` → 回源到 `danmo-admin` 或 `danmo-user`（HTTPS，内网地址）。
   - 其余路径 → `index.html`（SPA fallback），`index.html` 短缓存或 no-cache。
3. 环境变量：
   - `CORS_ORIGINS=https://admin.example.com`（用户 Portal 填 user 域名；逗号分隔多个 Origin）。
   - 用户中心展示 relay 地址：`PUBLIC_RELAY_URL=https://api.example.com/v1`。

**Nginx 同源示例（管理 Portal 域名）**

```nginx
server {
    listen 443 ssl http2;
    server_name admin.example.com;

    root /var/www/danmo-admin;
    index index.html;

    location /assets/ {
        expires 30d;
        add_header Cache-Control "public, immutable";
    }

    location /api/ {
        proxy_pass http://10.0.1.20:3001;   # 内网 admin API
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location / {
        try_files $uri $uri/ /index.html;
    }
}
```

用户 Portal 同理，`proxy_pass` 指向 `:3002`。

### 部署方式 B — 与 API 同机 Nginx

单台或少量 VM：同一 Nginx 既 `root` 静态目录，又反代 `/api` 与（可选）`/v1` 到 relay。适合小规模；转发量大时仍建议 relay 独立池 + L4。

### 部署方式 C — Kubernetes

| 组件 | 建议 |
|------|------|
| relay | `Deployment` + **Service type LoadBalancer**（云 LB 多为 L4）或 `NodePort` + 云 L4；`livenessProbe`/`readinessProbe`：`httpGet` `/healthz` port `3000` |
| admin / user | `ClusterIP` + Ingress（七层）仅暴露 Portal 域名；或 Portal 静态放对象存储，Ingress 只接 `/api` |
| 配置 | `Secret` 存 `ENCRYPTION_KEY`、数据库 URL；ConfigMap 存非敏感 env |
| 存储 | SQLite 需 **ReadWriteOnce** 卷且 **单副本写**；多 relay 副本请规划网络 DB |

Ingress **不适合** 作为 relay 主入口（七层流式限制）；relay 用 LoadBalancer Service 或 L4 chart。

## 云厂商对照（名称速查）

以下为常见产品 **类型** 对照，控制台名称以各云文档为准。

| 能力 | AWS | 阿里云 | 腾讯云 | Google Cloud | Azure |
|------|-----|--------|--------|--------------|-------|
| 四层公网 LB | NLB | SLB（传统/NLB 实例） | CLB / 网络型 | External TCP/UDP LB | Azure Load Balancer |
| 七层 / CDN | ALB + CloudFront | ALB + CDN | CLB 七层 + CDN | Cloud CDN + LB | App Gateway + Front Door |
| 对象存储静态 | S3 | OSS | COS | GCS | Blob + CDN |
| 证书 | ACM | 证书服务 | SSL 证书 | Certificate Manager | Key Vault / App Service 证书 |

**Relay 目标组示例逻辑（AWS NLB）**：Target type `instance` → Port `3000` → Health check HTTP `/healthz` → Deregistration delay 300–600s。

## 域名与环境变量对照

| 公网域名 | 后端 | 备注 |
|----------|------|------|
| `api.example.com` | L4 → relay `:3000` | SDK / 用户 `PUBLIC_RELAY_URL` |
| `admin.example.com` | 静态 + `/api` → admin | `CORS_ORIGINS` 含此 Origin |
| `user.example.com` | 静态 + `/api` → user | 同上 |

```bash
# /opt/danmo/.env（节选）
CORS_ORIGINS=https://admin.example.com,https://user.example.com
PUBLIC_RELAY_URL=https://api.example.com/v1
SEED_DEMO=0
```

## 验收清单

- [ ] L4 池仅暴露 relay；`/healthz`  unhealthy 实例会自动摘除
- [ ] 流式：`curl -N` Chat SSE 经公网域名可持续数分钟不断
- [ ] Portal 登录后会话保持（同源 `/api`，Cookie `danmo_session`）
- [ ] 用户中心显示的 API 地址与 `PUBLIC_RELAY_URL` 一致
- [ ] `make package` 或 CI 构建版本与运行二进制 tag 一致

## 相关文档

- [operations.md](operations.md) — 环境变量、systemd、备份、排障
- [compliance.md](compliance.md) — 部署前合规与 `VITE_COMPLIANCE_URL`
- [channel-health.md](channel-health.md) — relay 渠道熔断（与 LB 健康检查无关，勿混淆）
