# 运维指南

本文是 danmo-router **生产与日常运维** 的统一入口：部署拓扑、配置、扩缩容、升级、备份、观测与常见故障处理。实现细节见 [系统架构](architecture.md)；渠道熔断与 429 降权见 [渠道健康与选路](channel-health.md)。

## 文档地图

| 主题 | 文档 |
|------|------|
| 协议、方言、计费、数据模型 | [architecture.md](architecture.md) |
| `CB_*` / `RL_*`、选路与熔断日志 | [channel-health.md](channel-health.md) |
| 与同类项目对比 | [comparison.md](comparison.md) |
| 合法授权、可接受使用与责任边界 | [compliance.md](compliance.md) |
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

Portal 静态资源：`apps/admin/dist`（5173）、`apps/user/dist`（5174），生产由 CDN / Nginx 托管。

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
- [ ] `DATABASE_URL` 指向持久化路径或计划中的 Postgres（当前代码仍仅支持 SQLite）
- [ ] `CORS_ORIGINS` 为 Portal 真实域名（逗号分隔，勿误用 `*` 与凭证 Cookie 组合）
- [ ] `PUBLIC_RELAY_URL` 设为用户中心展示的 API 根（含 `/v1`，如 `https://api.example.com/v1`）
- [ ] 构建 Portal 时按需设置 `VITE_COMPLIANCE_URL`（默认链到仓库 [compliance.md](compliance.md)；自托管时指向你的合规页 URL）
- [ ] 反代对 relay **关闭响应缓冲**（流式 `SSE` / Gemini 流）
- [ ] 负载均衡健康检查指向各进程的 `GET /healthz`
- [ ] `RUST_LOG` 至少为 `info`；排障时可临时 `danmo_relay=debug`（模块名以 crate 为准，见下文日志）

## 构建与发布产物

**本地打包（二进制 + Portal + `.env.example`）：**

```bash
make package
# 产出 dist/bin/{danmo-relay,danmo-admin,danmo-user} 与 dist/apps/{admin,user}
```

**Release：** 推送 `v*` tag 触发 [`.github/workflows/release.yml`](../.github/workflows/release.yml)，GitHub Release 含多平台二进制 tar 与 Portal tar。

**质量门禁（与 CI 一致）：**

```bash
make check   # fmt + clippy + test
```

## 生产部署（拆分）

1. 将 `dist/bin` 安装到统一路径（如 `/opt/danmo/bin`），`.env` 放在 `/opt/danmo/.env`。
2. 三组 systemd（或容器）进程共享同一 `EnvironmentFile`，仅 `PORT` 与 `ExecStart` 二进制不同。
3. 反代示例：

| 公网域名 | 后端 | 注意 |
|----------|------|------|
| `api.example.com` | `danmo-relay:3000` | `proxy_buffering off`、足够长的读超时 |
| `admin-api.example.com` | `danmo-admin:3001` | 限制来源 IP |
| `user-api.example.com` | `danmo-user:3002` | |
| 管理 Portal | 静态 `admin/dist`，`/api` → admin API | |
| 用户 Portal | 静态 `user/dist`，`/api` → user API | |

**systemd 示例（relay）：**

```ini
[Service]
EnvironmentFile=/opt/danmo/.env
Environment=PORT=3000
ExecStart=/opt/danmo/bin/danmo-relay
Restart=always
RestartSec=3
```

admin / user 同理，改 `PORT` 与 `ExecStart` 中的二进制名。

**Nginx relay 片段：**

```nginx
location / {
    proxy_pass http://127.0.0.1:3000;
    proxy_http_version 1.1;
    proxy_set_header Connection "";
    proxy_buffering off;
    proxy_read_timeout 3600s;
    proxy_send_timeout 3600s;
}
```

## 环境变量速查

完整模板见 [`.env.example`](../.env.example)。下表按运维场景分组；`CB_*` / `RL_*` 行为说明见 [channel-health.md#环境变量](channel-health.md#环境变量)。

### 通用

| 变量 | 默认 | 说明 |
|------|------|------|
| `HOST` | `0.0.0.0` | 监听地址 |
| `PORT` | 见上表 | 未设置时用各二进制默认端口 |
| `DATABASE_URL` | `sqlite://data/danmo.db?mode=rwc` | 共享库；WAL 模式，多进程可读写的轻量场景 |
| `ENCRYPTION_KEY` | 开发用固定值 | 渠道 Key 加密与 API Key HMAC；**变更后已存渠道密文不可解密** |
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

**渠道健康状态** 也在各 relay 进程内存中，副本间短暂不一致属预期；见 [channel-health.md](channel-health.md)。

## 健康检查与观测

### HTTP

所有二进制均注册：

```http
GET /healthz
```

响应体为纯文本 `ok`。负载均衡探活建议使用此路径，而非 `/v1/models`（需令牌）。

### 日志

- 启动：`{service} listening on http://{addr}`
- 数据库：`applied migration {version}`（升级后首次启动）
- Relay 渠道健康（`RUST_LOG=info` 可见），详见 [channel-health.md#运维提示](channel-health.md#运维提示)：
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

1. **备份数据库**（见下一节）。
2. 滚动替换二进制：建议顺序 **admin/user → relay**，或先扩新 relay 再摘旧实例，避免长时间全站 relay 不可用。
3. 任一进程启动时会执行迁移：版本记录在 `schema_migrations`，每条 SQL 迁移**只应用一次**。
4. 观察日志是否出现新的 `applied migration N`；若启动失败，用备份回滚二进制与库文件。

**版本兼容：** 同一发行版内 admin、relay、user 应使用**相同 git/tag 构建**，避免 schema 与 API 不一致。

## 备份与恢复

**SQLite：**

```bash
# 推荐在业务低峰、且最好仅一个写进程时拷贝；或使用 sqlite3 .backup
cp /path/to/danmo.db /backup/danmo-$(date +%F).db
# WAL 模式下可同时保留 danmo.db-wal / danmo.db-shm，或 checkpoint 后单文件拷贝
```

**必须一并安全保管：**

- `ENCRYPTION_KEY`（丢失则渠道上游 Key 无法解密）
- `.env` 中的管理员与 CORS 等配置

**恢复：** 停所有进程 → 还原 `danmo.db` → 确认 `ENCRYPTION_KEY` 与备份时一致 → 按部署章节启动。

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
| 429 / 5xx 上游 | 渠道限流或故障 | 看 relay 日志与 [渠道健康](channel-health.md)；管理端测试渠道；临时禁用坏渠道 |
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
| 发布新版本 | tag → Release 产物 → 备份 → 滚动升级 |
| 本地复现 | `SEED_DEMO=1` + [README 本地开发](../README.md#本地开发) |

## 相关命令

```bash
make help
make package          # 生产目录布局
make run-relay        # 本地 relay :3000
make run-admin        # 本地 admin :3001
make run-user         # 本地 user :3002
make portals-build    # 构建 Portal 静态文件
make check            # 与 CI 相同检查
```

---

变更架构或运维行为时，请同步更新本文、[architecture.md](architecture.md) 与 [channel-health.md](channel-health.md)，并在 [README.md](../README.md) 保留指向本文的链接。
