# 云平台部署指南

本文按**云厂商平台**给出 danmo-router 的参考部署方案：计算实例跑二进制、**四层负载均衡**承接转发流量、**HTTP `/healthz` 健康检查**、**对象存储 + CDN** 发布 Portal（同源 `/api` 回源到 admin/user）。环境变量、备份与排障见 [operations.md](operations.md)。

## 部署共性（各平台相同部分）

### 组件与域名

| 组件 | 进程/产物 | 典型端口 | 公网域名示例 |
|------|-----------|----------|--------------|
| 转发 | `danmo-relay` | 3000 | `api.example.com` |
| 管理 API | `danmo-admin` | 3001 | 不直连公网，由 Portal 回源 |
| 用户 API | `danmo-user` | 3002 | 不直连公网，由 Portal 回源 |
| 管理 Portal | `apps/admin/dist` | — | `admin.example.com` |
| 用户 Portal | `apps/user/dist` | — | `user.example.com` |

```text
api.example.com     ──► 云厂商「四层 LB」──► relay ECS/CVM × N :3000
admin.example.com   ──► CDN ──► OSS/COS（静态）+ 回源 /api ──► admin :3001（内网/VPC）
user.example.com    ──► CDN ──► OSS/COS（静态）+ 回源 /api ──► user  :3002（内网/VPC）
```

### 实例内进程（systemd）

在每台计算实例安装 Release 二进制（`make package` 或 GitHub Release），共享 `/opt/danmo/.env`（`DATABASE_URL`、`ENCRYPTION_KEY` 一致）。relay 节点只跑 relay；管理面可 1～2 台跑 admin + user（或拆分）。

```ini
# /etc/systemd/system/danmo-relay.service
[Service]
EnvironmentFile=/opt/danmo/.env
Environment=PORT=3000
ExecStart=/opt/danmo/bin/danmo-relay
Restart=always
```

admin / user 同理，改 `PORT` 与二进制名。SQLite 库文件需放在**共享 NAS/云盘**或仅单写节点（多 relay 时注意 [operations.md — 扩缩容](operations.md#扩缩容与容量)）。

### 健康检查（各平台控制台填法一致）

应用探针（非 `/v1/models`）：

```http
GET /healthz  →  200，body: ok
```

| 字段 | relay | admin | user |
|------|-------|-------|------|
| 协议 | HTTP | HTTP | HTTP |
| 端口 | 3000 | 3001 | 3002 |
| 路径 | `/healthz` | `/healthz` | `/healthz` |
| 间隔 | 5–10s | 10s | 10s |
| 不健康阈值 | 3 次 | 3 次 | 3 次 |

### Portal 构建与 Cookie

```bash
cd apps/admin && npm ci && VITE_COMPLIANCE_URL=https://你的合规页 npm run build
cd apps/user  && npm ci && VITE_COMPLIANCE_URL=https://你的合规页 npm run build
```

前端使用相对路径 `/api/*` 且 `credentials: 'include'`，**务必**让浏览器看到的 Portal 与 `/api` **同域名**（CDN 路径回源或七层反代），并在 `.env` 设置：

```bash
CORS_ORIGINS=https://admin.example.com,https://user.example.com
PUBLIC_RELAY_URL=https://api.example.com/v1
SEED_DEMO=0
```

### 转发面为何用四层 LB

Relay 含长连接 SSE/流式；四层 TCP 分发不在网关缓冲 HTTP body。TLS 推荐：**四层 LB TCP:443 → 实例 Nginx:443 终结证书 → `127.0.0.1:3000`**，Nginx 需 `proxy_buffering off`、读超时 ≥3600s（片段见 [operations.md](operations.md#生产部署拆分)）。

---

## 阿里云

### 推荐产品组合

| 用途 | 产品 |
|------|------|
| relay 计算 | ECS（或 ACK 节点） |
| 转发入口 | **NLB（网络型负载均衡）** TCP 监听器 |
| 管理/用户计算 | ECS（可与 relay 分机部署） |
| Portal 静态 | **OSS** + **CDN**（或 **DCDN** 全站加速） |
| 证书 | 数字证书管理服务（部署在 Nginx 或 DCDN） |
| 数据库文件 | ECS 云盘 / NAS 共享挂载（SQLite） |

### 方案 A：ECS + NLB（转发）+ OSS/CDN（Portal）

**1. VPC 与安全组**

- 创建 VPC，relay ECS 置于多可用区交换机。
- 安全组 `sg-relay`：入站仅 **NLB 健康检查网段** + **NLB 所在子网** 访问 `TCP 3000`（若 Nginx 443 则放行 443）；**禁止** 0.0.0.0/0 直连 3001/3002。
- 安全组 `sg-admin`：入站仅 CDN/ALB 回源网段或 VPC 内 `3001/3002`。

**2. NLB 四层转发（`api.example.com`）**

| 配置项 | 值 |
|--------|-----|
| 负载均衡类型 | NLB |
| 监听 | TCP 443（或 TCP 80 内网测试） |
| 后端服务器组 | 多台 relay ECS |
| 后端端口 | 443（Nginx）或 3000（直连 relay，仅内网） |
| 调度 | 加权轮询 |
| 健康检查 | HTTP，端口 **3000**，路径 **`/healthz`**，正常状态码 200 |
| 连接空闲超时 | 按控制台上限调大（流式场景） |

DNS：`api.example.com` CNAME 到 NLB 提供的域名。

**3. relay ECS 上 Nginx（443 → 3000）**

- 绑定证书，反代到 `127.0.0.1:3000`，关闭缓冲、拉长超时。
- NLB 健康检查仍打 **3000** `/healthz`（绕过 TLS 更简单）。

**4. admin / user ECS**

- 各跑 `danmo-admin`、`danmo-user`，监听内网 IP。
- 无需公网 IP；通过 CDN 回源或内网 ALB 访问。

**5. Portal：OSS + CDN**

| 步骤 | 操作 |
|------|------|
| 上传 | 将 `admin/dist`、`user/dist` 分别传到两个 Bucket（或前缀 `admin/`、`user/`） |
| 域名 | CDN 加速域名 `admin.example.com`、`user.example.com`，源站选 OSS |
| SPA | OSS 静态页或 CDN **回源规则**：除 `/assets/*` 外 404 回源到 `/index.html`（或使用 CDN 配置「单页应用」类规则） |
| `/api` 回源 | CDN **路径回源**或 **回源 HOST + 源站**：路径 `/api/*` 指向 **admin/user 源站**（内网 ALB 七层、或带公网 IP 的 ECS 仅允许 CDN 回源 IP） |
| 缓存 | `/assets/*` 长缓存；`/api/*` **不缓存**；`index.html` 短缓存 |

若 CDN 路径回源配置复杂，可改用 **ALB（七层）** 作为 Portal 域名源站：默认服务器组指向 OSS 静态（或通过 ECS Nginx `root`），转发规则 `/api/*` 指向 admin/user 后端组。

**6. 验收**

- NLB 后端 unhealthy 实例自动摘除。
- `curl -N https://api.example.com/v1/chat/completions ...` 流式不断。
- 浏览器登录 admin Portal 后会话保持。

---

## 腾讯云

### 推荐产品组合

| 用途 | 产品 |
|------|------|
| relay 计算 | CVM |
| 转发入口 | **负载均衡 CLB — 网络负载均衡（NetWork LB）** TCP 监听 |
| Portal | **COS** + **CDN** |
| 证书 | SSL 证书（托管到 Nginx 或 CDN） |

### 方案：CVM + 网络 CLB + COS/CDN

**1. 网络 CLB（转发）**

| 配置项 | 值 |
|--------|-----|
| 实例类型 | 网络负载均衡 |
| 监听器 | TCP:443 |
| 绑定 | 多个 relay CVM 私网 IP |
| 后端端口 | 443（Nginx）或 3000 |
| 健康检查 | HTTP，检查端口 **3000**，检查路径 **`/healthz`**，HTTP 请求方法 GET |
| 会话保持 | 关闭（或按需开启源 IP 会话，长连接场景通常单连接已固定后端） |

**2. CVM 安全组**

- `relay-sg`：仅 CLB 健康探测与 CLB 网段访问 3000/443。
- `portal-api-sg`：仅 CDN 回源地址段访问 3001/3002（参考 [CDN 回源 IP 段文档](https://cloud.tencent.com/document/product/228) 定期更新）。

**3. COS + CDN（Portal）**

| 步骤 | 操作 |
|------|------|
| 存储桶 | 私有读写，开启静态网站（索引 `index.html`） |
| CDN 域名 | 绑定 `admin.example.com` / `user.example.com` |
| 回源 | 默认 COS；**高级配置** 中对 `/api/` 配置 **源站为 admin/user CVM 内网 CLB（七层）或 CVM IP** |
| HTTPS | CDN 配置证书，回源协议 HTTPS 或 HTTP（内网） |

**4. 可选：七层 CLB 统一 Portal**

单 CLB 多监听器：443 上 `admin.example.com` → 静态 CVM（Nginx root）+ location `/api/` → 3001；`user.example.com` 同理。COS 仅作构建物发布介质，由流水线同步到 CVM。

---

## 华为云

### 推荐产品组合

| 用途 | 产品 |
|------|------|
| relay | ECS |
| 转发入口 | **ELB 独享型 / 共享型 — TCP 监听器**（四层） |
| Portal | **OBS** + **CDN** |
| 证书 | 云证书服务（CCM） |

### 方案：ECS + ELB（TCP）+ OBS/CDN

**1. ELB 四层**

| 配置项 | 值 |
|--------|-----|
| 前端协议/端口 | TCP 443 |
| 后端组 | relay ECS，端口 443 或 3000 |
| 健康检查 | **HTTP**，端口 **3000**，URL **`/healthz`**，期望 2xx |
| 跨AZ | 开启 |

**2. 安全组**

- 入站：ELB 子网 → ECS 3000/443；禁止公网直连 admin/user。

**3. OBS + CDN**

- 上传构建产物到 OBS；CDN 加速域名指向 OBS。
- 配置 **回源 URL 改写 / 多源站**：`/api/*` 回源至 admin/user ECS 私网地址或后端 ELB（七层 HTTP/HTTPS 监听器）。
- SPA：`错误码 404` 回源 OBS 的 `index.html`（按华为云 CDN 控制台「自定义错误页面」配置）。

---

## 火山引擎（Volcengine）

### 推荐产品组合

| 用途 | 产品 |
|------|------|
| relay | 云服务器 ECS |
| 转发入口 | **负载均衡 CLB — 四层（TCP/UDP）** |
| Portal | **TOS** + **CDN** |

### 方案：ECS + 四层 CLB + TOS/CDN

**1. 四层 CLB**

| 配置项 | 值 |
|--------|-----|
| 监听协议 | TCP |
| 端口 | 443 |
| 后端服务器 | relay ECS 实例组 |
| 健康检查 | HTTP，`/healthz`，端口 **3000** |

**2. TOS + CDN**

- 桶开启静态网站；CDN 域名 CNAME。
- 「回源配置」中为路径 `/api/` 指定独立源站（admin/user 内网 ECS 或内网 CLB 七层）。
- 流式 API 域名 `api.example.com` 解析到 **四层 CLB**，不要经 CDN（避免缓存与断流）。

---

## AWS

### 推荐产品组合

| 用途 | 产品 |
|------|------|
| relay | EC2 或 EKS worker |
| 转发入口 | **Network Load Balancer (NLB)** |
| Portal | **S3** + **CloudFront** |
| 证书 | **ACM**（CloudFront / ALB） |

### 方案：EC2 + NLB + CloudFront

**1. NLB**

| 配置项 | 值 |
|--------|-----|
| Scheme | internet-facing |
| Listener | TCP:443 |
| Target group | Instance type，port **3000**（或 443→Nginx） |
| Health check protocol | HTTP |
| Health check path | `/healthz` |
| Success codes | 200 |
| Deregistration delay | 300–600s（便于流式连接 drain） |

**2. EC2 安全组**

- `relay-sg`：Inbound 3000/443 **only from NLB node SG**（使用 [NLB 作为 target 时参考 AWS 文档配置 health check 与 client IP](https://docs.aws.amazon.com/elasticloadbalancing/latest/network/load-balancer-targets.html)）。

**3. CloudFront + S3（Portal）**

| 行为 | 配置 |
|------|------|
| Origin 1 | S3 bucket（OAC），默认行为，静态资源 |
| Origin 2 | admin/user **内网 ALB** 或 EC2（仅安全组允许 CloudFront 前缀列表或 VPC Origin） |
| Cache behavior | `/api/*` → Origin 2，**CachingDisabled**，允许所有 HTTP 方法 |
| Default | `/*` → S3，`index.html` 用 CloudFront Function 或 Custom error 403→200 返回 `/index.html`（SPA） |
| API 域名 | 单独 `api.example.com` → NLB，**不要**走 CloudFront |

**4. EKS 变体（简述）**

- relay：`Service type=LoadBalancer`（provision NLB），`readinessProbe` `httpGet: /healthz:3000`。
- admin/user：`ClusterIP` + Ingress（ALB Ingress Controller）供 CloudFront VPC Origin 或内网 ALB 回源。

---

## Google Cloud（GCP）

| 用途 | 产品 |
|------|------|
| relay | Compute Engine MIG |
| 四层 LB | **External TCP/UDP Network Load Balancing** |
| Portal | **Cloud Storage** + **Cloud CDN**（后端 bucket 或 LB） |
| 七层 Portal `/api` | **External Application Load Balancer** URL map：`/api/*` → admin/user backend service |

**Network LB**：Health check 选 HTTP，port 3000，request path `/healthz`。Forwarding rule TCP 443 → relay MIG。

**Portal**：Cloud CDN 启用在 ALB 或 bucket 上；`/api/*` 指向 serverless NEG 或 GCE 实例组（admin/user）。

---

## Microsoft Azure

| 用途 | 产品 |
|------|------|
| relay | Virtual Machine Scale Set (VMSS) |
| 四层 LB | **Azure Load Balancer**（Standard SKU）TCP 443 |
| 健康探测 | HTTP `/healthz` 需 **Load Balancer 规则 + 或 VM 内 Nginx**；Standard LB 支持 HTTP probe 到 port 3000 |
| Portal | **Storage Static Website** + **Azure Front Door** 或 **Application Gateway** |

Front Door：`/api/*` route 到 admin/user 源（Private Link 或公网 IP 限制）；静态 route 到 Blob static website。`api.example.com` 用 **Load Balancer** 公网 IP 指向 relay VMSS。

---

## 平台选型速查

| 平台 | 四层转发产品 | 静态 Portal | 路径 `/api` 回源常见做法 |
|------|--------------|-------------|---------------------------|
| 阿里云 | NLB TCP | OSS + CDN | CDN 路径回源 / DCDN / ALB 七层 |
| 腾讯云 | 网络 CLB TCP | COS + CDN | CDN 多源站 / 七层 CLB |
| 华为云 | ELB TCP | OBS + CDN | CDN 回源到内网 ELB/ECS |
| 火山引擎 | CLB 四层 | TOS + CDN | CDN 独立源站 |
| AWS | NLB | S3 + CloudFront | CloudFront 第二 Origin |
| GCP | Network LB | GCS + CDN | ALB URL map |
| Azure | Load Balancer | Blob + Front Door | Front Door route |

## 验收清单（各平台通用）

- [ ] 仅 **四层 LB** 对公网暴露 relay；3001/3002 不对 0.0.0.0/0 开放
- [ ] 目标组/后端组 HTTP 探活 **`/healthz`** 生效
- [ ] `api.example.com` **不经过** CDN
- [ ] Portal 登录 Cookie 正常（同源 `/api`）
- [ ] `PUBLIC_RELAY_URL` 与 NLB/CLB 域名一致
- [ ] 合规页 `VITE_COMPLIANCE_URL` 已按 [compliance.md](compliance.md) 配置

## 相关文档

- [operations.md](operations.md) — 环境变量、备份、升级、Nginx relay 片段
- [compliance.md](compliance.md) — 部署前合规
- [channel-health.md](channel-health.md) — relay 进程内渠道熔断（非 LB 探活）
