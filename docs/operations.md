# 运维手册

本文是 danmo-router **部署与日常运维** 的统一入口：自建与**云厂商平台**部署、配置、扩缩容、升级、备份、观测与排障。协议、转发、计费、渠道熔断等设计见 [系统架构](architecture.md)。

## 文档地图

| 主题 | 文档 |
|------|------|
| 系统设计（协议、方言、计费、渠道健康、`CB_*`/`RL_*`） | [architecture.md](architecture.md) |
| 与同类项目对比 | [comparison.md](comparison.md) |
| 合法授权、可接受使用与责任边界 | [compliance.md](compliance.md) |
| Compose 首次发布 / 升级、edge Nginx | [deploy/README.md](../deploy/README.md)（`install.sh` / `upgrade.sh`） |
| Agent 与迁移规则 | [AGENTS.md](../AGENTS.md) |
| 本地开发与 Makefile | [README.md](../README.md) |
| 环境变量模板 | [`.env.example`](../.env.example) |

## 组件与端口

三个二进制共用同一 `DATABASE_URL` 与 `ENCRYPTION_KEY`，可独立部署：

| 二进制 | 默认端口 | 职责 | 扩容建议 |
|--------|----------|------|----------|
| `danmo-relay` | 3000 | 鉴权、路由、转发、计费 | **优先水平扩容** |
| `danmo-admin` | 3001 | 管理 API | 通常 1～少量副本 |
| `danmo-user` | 3002 | 用户 API | 通常 1～少量副本 |
| `danmo-router` | 3000 | 本地一体进程 | **勿用于生产** |

Portal 静态资源：开发时 `apps/admin`（5173）、`apps/user`（5174）；生产打进 **`danmo-edge`**，由 Compose Nginx 同域托管（不走 OSS/CDN）。

```text
Client ──► LB ──► danmo-relay × N ──► upstream LLM
Admin UI ──────► danmo-admin
User UI  ──────► danmo-user
                      │
                      └── 共享 DB（当前 SQLite WAL；多 relay 副本宜换网络 DB）
```

## 上线前检查清单

- [ ] `SEED_DEMO=0`（生产禁止示例数据）
- [ ] `ENCRYPTION_KEY` 为随机 64 位十六进制（32 字节），**所有进程一致**；勿使用 `.env.example` 默认值
- [ ] `ADMIN_USERNAME` / `ADMIN_PASSWORD` 已修改；管理 API 仅内网或 VPN 可达
- [ ] `DATABASE_URL` 指向持久化路径（当前 **sqlite://**；`mysql://` 即将支持，见 `migrations/mysql/`）
- [ ] `CORS_ORIGINS` 为 Portal 真实域名（逗号分隔，勿误用 `*` 与凭证 Cookie 组合）
- [ ] `PUBLIC_RELAY_URL` 设为用户中心展示的 API 根（含 `/v1`，如 `https://api.example.com/v1`）
- [ ] 构建 Portal 时按需设置 `VITE_COMPLIANCE_URL`（默认链到仓库 [compliance.md](compliance.md)；自托管时指向你的合规页 URL）
- [ ] 反代对 relay **关闭响应缓冲**（流式 `SSE` / Gemini 流）
- [ ] 负载均衡健康检查指向各进程的 `GET /healthz`
- [ ] `RUST_LOG` 至少为 `info`；排障时可临时 `danmo_relay=debug`（模块名以 crate 为准，见下文日志）

## 构建与发布产物

**整栈（与生产同拓扑）：**

```bash
cp .env.example .env   # 修改密钥与域名
make compose-up        # 或 ./deploy/scripts/install.sh
```

**本地镜像构建：** `make docker-build`（四 target：relay/admin/user/edge）。

**Release：** 推送 `v*` tag → [`.github/workflows/release.yml`](../.github/workflows/release.yml) 向 **GHCR** 推送四镜像（见 [`deploy/RELEASE-LAYOUT.md`](../deploy/RELEASE-LAYOUT.md)）。**不再**发布平台二进制 tar 或 portals tar。

开发 / 单测仍用 `make check`、`make run-*`（cargo），不必打 Release 包。

**质量门禁（与 CI 一致）：**

```bash
make check   # fmt + clippy + test
```

## 生产部署（Compose）

**TLS：** 后端仅 HTTP。公网 HTTPS 由 **edge Nginx**（`danmo-edge`）或云四层/七层 LB 终结；不要在 relay 进程内做 TLS。

- **首次发布：** [`deploy/scripts/install.sh`](../deploy/scripts/install.sh) — 更新 `.env`、按 `--role=` 选择 Compose profiles、`docker compose up -d`。证书未齐时用 `ENABLE_TLS=0`（LB 终结）或准备好 `./certs` 后加 `--enable-tls`。
- **升级：** [`deploy/scripts/upgrade.sh`](../deploy/scripts/upgrade.sh) — 默认 `pull` + recreate；`--build` 从源码重建。增量 SQL 由进程 `migrate()` 执行。
- **Nginx 模板：** [`deploy/compose/nginx/`](../deploy/compose/nginx/)。旧主机脚本 `setup-nginx.sh` 已废弃。

```text
Client ──► [可选四层 LB] ──► edge:80/443 ──HTTP──► relay|admin|user（Compose 内网）
```

| 公网域名 | edge 行为 | 注意 |
|----------|-----------|------|
| `api.example.com` | 反代 → `relay:3000` | `proxy_buffering off`、长读超时 |
| `admin.example.com` | 静态 admin + `/api` → `admin:3001` | 与 Portal 同域 Cookie |
| `user.example.com` | 静态 user + `/api` → `user:3002` | |

**仅 edge 映射宿主机端口**；Compose 中 API 容器与 edge 共享网络命名空间（`127.0.0.1:300x`），勿再单独发布 3000/3001/3002。云 Serverless 分任务部署时改用平台内网 DNS，并设置 `RELAY_UPSTREAM` / `ADMIN_UPSTREAM` / `USER_UPSTREAM`。

## 云厂商平台部署

两条路径共用 GHCR 镜像（`danmo-relay` / `danmo-admin` / `danmo-user` / `danmo-edge`）：

1. **云本机 Compose**：ECS/CVM 安装 Docker → `install.sh` / `compose up`；LB → 主机 edge:80/443。
2. **云 Serverless 容器**：平台拉取同一镜像（阿里云 SAE、腾讯云容器型、AWS App Runner / ECS Fargate 等）；配置探活、长超时、持久卷；Portal 仍跑 **edge**，**不走 OSS/CDN**。

### 部署共性

| 组件 | 镜像 / 进程 | 典型端口 | 公网域名示例 |
|------|-------------|----------|--------------|
| 转发 | `danmo-relay` | 3000（内网） | 经 edge：`api.example.com` |
| 管理 API | `danmo-admin` | 3001（内网） | 经 edge `/api` |
| 用户 API | `danmo-user` | 3002（内网） | 经 edge `/api` |
| 边缘 | `danmo-edge` | 80/443 | api / admin / user 域名 |

```text
api.example.com     ──► 云 LB ──► edge ──► relay
admin.example.com   ──► 云 LB ──► edge（静态 admin + /api → admin）
user.example.com    ──► 云 LB ──► edge（静态 user  + /api → user）
```

**数据：** SQLite 使用 Compose 命名卷或云盘挂载到 `/data`；多 relay 副本仍受 SQLite 限制（见 [扩缩容与容量](#扩缩容与容量)）。Serverless 须挂载持久卷，不宜无状态冷启动 + 本地盘。

**LB 探活：** HTTP `/healthz`。Compose 单机可探活 edge（`Host: api…`）或直连任务端口 3000（若平台把 relay 单独暴露在 VPC 内）。

| 字段 | 建议 |
|------|------|
| 协议 | HTTP |
| 路径 | `/healthz` |
| 超时 | relay 流式场景尽量加大空闲超时 |

**Portal：** 流量小，静态打进 `danmo-edge`，与 `/api` 同域；**不要**再配 OSS/COS + CDN 托管 Portal。

### 阿里云

| 用途 | 产品 |
|------|------|
| 计算 | ECS + Docker Compose，或 **SAE** / ACK 跑容器 |
| 入口 | **NLB** TCP 80/443 → edge |
| 证书 | 数字证书管理服务（LB 或 edge） |

SAE/ACK：分别部署四镜像或至少 `relay`+`edge`；共享 NAS/云盘给 SQLite；安全组勿对公网开放 3001/3002。

### 腾讯云

| 用途 | 产品 |
|------|------|
| 计算 | CVM + Compose，或容器型服务 / TKE |
| 入口 | **网络 CLB** → edge |

健康检查 HTTP `/healthz`。Portal 经 edge，无需 COS+CDN。

### 华为云

**ELB** → ECS Compose 或云容器实例上的 edge；持久化云硬盘挂载 `/data`。

### 火山引擎

**四层 CLB** → edge；TOS/CDN **不必**用于 Portal。

### AWS

**NLB** 或 ALB → ECS Fargate / App Runner 上的 edge；relay/admin/user 同 VPC 服务发现或 Compose on EC2。EFS/本地卷挂载 SQLite。App Runner / Fargate：拉长请求超时以支持 SSE。

### Google Cloud / Azure

GCP：Cloud Run / GCE + Compose；Azure：Container Apps / VM + Compose。LB 探活 `/healthz`；Portal 走 edge。

### 平台选型速查

| 平台 | 本机 Compose | Serverless 容器 | 入口 |
|------|--------------|-----------------|------|
| 阿里云 | ECS + Docker | SAE / ACK | NLB → edge |
| 腾讯云 | CVM + Docker | 容器服务 / TKE | CLB → edge |
| 华为云 | ECS + Docker | 云容器实例 | ELB → edge |
| 火山引擎 | 主机 + Docker | 容器服务 | CLB → edge |
| AWS | EC2 + Docker | App Runner / Fargate | NLB/ALB → edge |
| GCP | GCE + Docker | Cloud Run | LB → edge |
| Azure | VM + Docker | Container Apps | LB → edge |

**验收：** 宿主机/节点仅 edge 端口对公网或 LB 开放；`/healthz` 经 edge 或内网探活成功；Portal 登录 Cookie 正常；`PUBLIC_RELAY_URL` 与对外 API 域名一致；未使用 OSS/CDN 托管 Portal。

## 环境变量速查

完整模板见 [`.env.example`](../.env.example)。下表按运维场景分组；`CB_*` / `RL_*` 语义见 [architecture.md — 渠道健康与选路](architecture.md#渠道健康与选路)。

### 通用

| 变量 | 默认 | 说明 |
|------|------|------|
| `HOST` | `0.0.0.0` | 监听地址 |
| `PORT` | 见上表 | 未设置时用各二进制默认端口 |
| `DATABASE_URL` | `sqlite://data/danmo.db?mode=rwc` | 共享逻辑库；现仅 sqlite。`mysql://` 即将支持（迁移见 `migrations/mysql/`） |
| `ENCRYPTION_KEY` | 开发用固定值 | 渠道 Key 加密、API Key HMAC、注册验证码密封；**变更后已存渠道密文不可解密** |
| `RUST_LOG` | `info` | `tracing` 过滤器，如 `info,danmo_relay=debug` |
| `CORS_ORIGINS` | localhost Portal | 逗号分隔 Origin |
| `SESSION_TTL_HOURS` | `168` | 管理/用户会话 TTL（小时） |

### 管理端种子账号

| 变量 | 默认 | 说明 |
|------|------|------|
| `ADMIN_USERNAME` | `admin` | 首次建库时创建的管理员用户名 |
| `ADMIN_PASSWORD` | `admin123` | 对应密码（仅当该用户名不存在时写入） |
| `DEFAULT_USER_QUOTA` | `500000` | 新用户默认额度 |
| `SEED_DEMO` | `0` | `1` 时首次空库写入示例渠道/用户/日志；生产必须为 `0` |

### Relay 转发与入口限额

限额在**单个 relay 进程内存**中统计，**不跨副本共享**（见 [architecture.md — 入口限额](architecture.md#入口限额)）。

| 变量 | 默认 | 说明 |
|------|------|------|
| `RELAY_MAX_RETRIES` | `2` | 单请求最多换渠道次数 |
| `RELAY_USER_CONCURRENCY` | `4` | 单账号在本进程的并发在途请求 |
| `RELAY_INSTANCE_CONCURRENCY` | `32` | 本进程总并发在途（已验证 Key） |
| `RELAY_USER_QPS` | `5` | 单账号在本进程的 QPS 令牌桶 |
| `RELAY_INSTANCE_QPS` | `50` | 本进程总 QPS |
| `RELAY_LEGACY_CONCURRENCY` | `4` | 旧式 `sk-`+48hex 未验证 Key 的并发上限 |
| `RELAY_LEGACY_QPS` | `10` | 旧式 Key 查库前 QPS |
| `PUBLIC_RELAY_URL` | 空 | 用户 Portal 展示的 API 地址 |

### 渠道健康（仅 relay 读取）

| 变量 | 默认 |
|------|------|
| `CB_ENABLED` | `true` |
| `CB_FAILURE_THRESHOLD` | `5` |
| `CB_BASE_EJECT_SECS` | `30` |
| `CB_MAX_EJECT_SECS` | `600` |
| `RL_WEIGHT_FLOOR` | `0.05` |
| `RL_RECOVER_SECS` | `60` |

## 扩缩容与容量

**优先扩 `danmo-relay`**：转发 CPU/网络 bound；admin/user 多为管理流量。

**SQLite 限制：**

- 启动时启用 **WAL**、`busy_timeout=5s`，适合 **少量** relay + admin + user 共写同一文件。
- **大量 relay 副本** 或高写入 QPS 时，应迁移到支持并发的网络数据库（代码中 Postgres 仍为 planned；当前 `DATABASE_URL=postgres://...` 会启动失败）。

**Per-process 限额：** 若前面有 4 个 relay，单账号「全局」并发约为 `4 × RELAY_USER_CONCURRENCY`（各副本独立计数）。需要集群级限流时要在 LB 或外部网关补充策略。

**渠道健康状态** 也在各 relay 进程内存中，副本间短暂不一致属预期；见 [architecture.md — 渠道健康与选路](architecture.md#渠道健康与选路)。

## 健康检查与观测

### HTTP

所有二进制均注册：

```http
GET /healthz
```

响应体为纯文本 `ok`。负载均衡探活建议使用此路径，而非 `/v1/models`（需令牌）。云平台探活参数见 [云厂商平台部署](#云厂商平台部署)。

### 日志

- 启动：`{service} listening on http://{addr}`
- 数据库：`applied migration {version}`（升级后首次启动）
- Relay **进程内渠道健康**（`RUST_LOG=info`，与 LB 探活无关；机制见 [architecture.md — 渠道健康与选路](architecture.md#渠道健康与选路)）：
  - `channel ejected`
  - `probing ejected channel`
  - `channel rate limited`
  - `every channel is ejected, ignoring circuit breakers`
  - `channel recovered`

**管理后台调用日志** 可筛 `success` / `billing_failed` / 上游错误；`billing_failed` 表示响应中无法解析用量，系统会退预扣额度。

**渠道连通性：** 管理 API `POST /api/admin/channels/{id}/test`（界面「测试」）请求上游 `{base_url}/models`，不计费。

### 建议的运维指标（需自行接入）

当前**无内置 Prometheus**；可在 LB / 反代侧采集：

- relay：`/healthz` 成功率、上游 5xx/429 率、P99 延迟
- 429 来自入口限额（客户端） vs 上游（渠道降权/熔断）需结合日志与调用日志区分

## 升级与数据库迁移

上线前勾选清单见上文「上线前检查清单」。

1. **备份数据库**（见下一节）。
2. **推荐：**

```bash
# 使用已发布的 GHCR tag
export DANMO_TAG=vX.Y.Z
./deploy/scripts/upgrade.sh

# 或从当前源码重建
./deploy/scripts/upgrade.sh --build
```

滚动时顺序建议先更新 admin/user，再更新 relay（或先扩新 relay 再摘旧实例）。

3. **不要**用脚本重跑全部 SQL。进程启动时对当前引擎目录（现 **sqlite**）只应用 `schema_migrations` 中尚未记录的版本；MySQL 接通后加载 `migrations/mysql/`。
4. 观察日志 `applied migration N`；失败则回滚镜像 tag 与库文件。

新增 schema：**必须**新增编号 SQL 文件（见 [AGENTS.md](../AGENTS.md) 与 [`crates/core/migrations/README.md`](../crates/core/migrations/README.md)），禁止改已发布的旧文件。

**版本兼容：** 同一发行版内 admin、relay、user、edge 应使用**相同 git/tag 构建**。

## 备份与恢复

**SQLite（Compose 卷 `/data`）：**

```bash
# 业务低峰导出命名卷（项目名默认 danmo → 卷 danmo_danmo-data）
docker compose run --rm --no-deps -v danmo_danmo-data:/data:ro alpine \
  tar -c -f - -C /data . > danmo-data-backup-$(date +%F).tar
# 或在仅一个写进程时拷贝卷内 danmo.db（含 -wal/-shm 或先 checkpoint）
```

**必须一并安全保管：**

- `ENCRYPTION_KEY`（丢失则渠道上游 Key 无法解密）
- `.env` 中的管理员与 CORS 等配置

**恢复：** `docker compose down` → 还原卷内数据库文件 → 确认 `ENCRYPTION_KEY` 一致 → `docker compose up -d`。

**演示数据重置：** 删除 SQLite 文件后设 `SEED_DEMO=1` 可重新生成示例（仅开发/演示环境）。

## 安全运维

| 项 | 说明 |
|----|------|
| `ENCRYPTION_KEY` | 生产随机生成；轮换需重新录入所有渠道上游 Key |
| 管理面 | 强密码、HTTPS、网络隔离；勿暴露默认 `admin123` |
| API Key | 用户令牌在用户中心轮换；旧式 hex Key 仍支持但入口限额更严 |
| 上游 Key | 存库加密；日志不应打印明文 Key |
| `SEED_DEMO` | 生产必须为 `0` |

## 故障排查（简表）

| 现象 | 可能原因 | 处理 |
|------|----------|------|
| 401 Invalid API key | 令牌错误、过期、禁用 | 用户中心检查令牌；管理端看用户/令牌状态 |
| 429 入口 | 超过 `RELAY_*` 并发或 QPS | 客户端降速；或提高单进程限额 / 增加 relay 副本（注意仍为 per-process） |
| 429 / 5xx 上游 | 渠道限流或故障 | 看 relay 日志与 [渠道健康](architecture.md#渠道健康与选路)；管理端测试渠道；临时禁用坏渠道 |
| 401 上游长期存在 | Key 无效 | **不会**自动熔断；需人工禁用或修正渠道 Key |
| 流式中断 / 客户端超时 | 反代缓冲或超时过短 | Nginx `proxy_buffering off`、加大 `proxy_read_timeout` |
| `billing_failed` | 上游响应无可用 usage 字段 | 检查模型/方言配置；见 [architecture.md — 用量与计费](architecture.md#用量与计费) |
| 多 relay 行为不一致 | 渠道健康与限额均 per-process | 预期行为；关键 Key 401/403 请人工禁用渠道 |
| 启动报 Postgres | 尚未支持 | 使用 `sqlite://` |
| `database is locked` | SQLite 争用过重 | 减少写并发进程数或迁移数据库 |

## 日常运维任务

| 任务 | 操作 |
|------|------|
| 新增上游渠道 | 管理后台 → 渠道 → 配置 base_url、协议、模型行与真实 Key |
| 调价 / 分组 | 模型价格、用户 `group_name`、abilities |
| 兑换码 | 管理后台生成；用户中心兑换 |
| 禁用渠道 | 管理 API / 界面改渠道 `status`（全实例立即生效，与内存熔断互补） |
| 发布新版本 | tag → GHCR 镜像 → 备份卷 → `upgrade.sh` |
| 本地复现 | `SEED_DEMO=1` + [README 本地开发](../README.md#本地开发) |

## 相关命令

```bash
make help
make check            # fmt + clippy + test
make docker-build     # 本地构建四镜像
make compose-up       # 整栈 Compose（需 Docker）
make run-relay        # 开发：cargo relay :3000
make run-admin        # 开发：API :3001 + Portal :5173
make run-user         # 开发：API :3002 + Portal :5174
```

---

变更架构或运维行为时，请同步更新本文与 [architecture.md](architecture.md)，并在 [README.md](../README.md) 保留指向本文的链接。
