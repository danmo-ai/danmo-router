# danmo-router

Fast LLM Router — OpenAI 兼容协议中转站，包含流量转发、管理 Portal、用户 Portal。

## 架构

三个可独立部署的二进制（共享同一 `DATABASE_URL`）：

| 二进制 | 默认端口 | 职责 |
|--------|----------|------|
| `danmo-relay` | 3000 | `/v1/*` 流量转发（可水平扩容） |
| `danmo-admin` | 3001 | `/api/admin/*` 管理 API |
| `danmo-user` | 3002 | `/api/user/*` 用户 API |
| `danmo-router` | 3000 | 本地一体进程（不建议生产） |

前端：`apps/admin`（5173）、`apps/user`（5174）。

```text
Client ──► LB ──► danmo-relay × N  ──► upstream LLM
Admin UI ──────► danmo-admin × 1..N
User UI  ──────► danmo-user  × 1..N
                      │
                      └── shared DB (SQLite WAL / 后续 Postgres)
```

扩容建议：优先扩 `danmo-relay`；admin/user 通常少量副本即可。多副本写库时请使用支持并发的数据库（当前 SQLite WAL 适合轻量多进程，不适合大量 relay 副本）。渠道上的方言决定请求按哪套厂商规范透传或转换，模型价格上的原厂方言决定客户端看到的字段。

## 界面

管理后台（`http://localhost:5173`，默认 `admin` / `admin123`）：

![概览](docs/images/admin-overview.png)

![渠道](docs/images/admin-channels.png)

![用户](docs/images/admin-users.png)

![模型价格](docs/images/admin-prices.png)

![调用日志](docs/images/admin-logs.png)

用户中心（`http://localhost:5174`，示例账号 `demo` / `demo1234`）：

![账户概览](docs/images/user-overview.png)

![API 令牌](docs/images/user-tokens.png)

![我的调用日志](docs/images/user-logs.png)

## Makefile

```bash
make help           # 查看全部目标
make test           # 测试
make release        # 编译三个拆分二进制 + all-in-one
make run-relay      # 启动转发 :3000
make run-admin      # 启动管理 API :3001
make run-user       # 启动用户 API :3002
make portals-build  # 构建两个 Portal
make package        # 产出 dist/bin + dist/apps
make check          # fmt + clippy + test（对齐 CI）
```

## 本地开发

```bash
cp .env.example .env

# 三个终端
make run-relay
make run-admin
make run-user

# Portal
make portals-install
make portals-dev-admin   # http://localhost:5173 → :3001
make portals-dev-user    # http://localhost:5174 → :3002
```

要在界面里看到渠道、用户和调用记录，把 `.env` 里的 `SEED_DEMO` 设为 `1` 后再启动。详见下一节。

一体进程（调试用）：

```bash
make run-all-in-one
# Portal 需改代理：VITE_API_PROXY=http://127.0.0.1:3000 npm run dev
```

## 模拟数据

`SEED_DEMO=1` 时，第一个连上数据库的进程会写入一份示例数据（只写一次；已有用户 `demo` 则跳过）。删掉 SQLite 文件后再启动即可重新生成。

| 账号 | 密码 | 说明 |
|------|------|------|
| `admin` | `admin123` | 管理后台，来自 `ADMIN_USERNAME` / `ADMIN_PASSWORD` |
| `demo` | `demo1234` | 用户中心，分组 `vip`，含 playground / batch / legacy 令牌 |
| `alice` | `alice123` | 普通用户 |
| `bob` | `bob12345` | 已禁用 |

示例令牌 `sk-demo-playground` 属于 `demo`。渠道里的上游 Key 都是占位符（`sk-demo-…-not-a-real-key`），不能打到真实厂商。示例里包含 OpenAI、Anthropic、DeepSeek、可协议转换的 Qwen、Gemini，以及一条停用的聚合渠道；价格表带原厂方言、输入分档和时段系数；日志里有成功、计费失败和上游错误。

生产环境保持 `SEED_DEMO=0`。

## 生产部署（拆分）

1. `make package` 或使用 GitHub Release 产物  
2. 三台/三组进程共用同一 `DATABASE_URL`、`ENCRYPTION_KEY`  
3. 反代：
   - `api.example.com` → `danmo-relay`（开启 `proxy_buffering off`）
   - `admin-api.example.com` → `danmo-admin`
   - `user-api.example.com` → `danmo-user`
4. 静态托管 `apps/admin/dist`、`apps/user/dist`，`/api` 指到对应 API  
5. `CORS_ORIGINS` 填 Portal 真实域名  

systemd 示例（relay）：

```ini
[Service]
EnvironmentFile=/opt/danmo/.env
Environment=PORT=3000
ExecStart=/opt/danmo/bin/danmo-relay
Restart=always
```

admin / user 同理，改 `PORT` 与二进制名。

## CI / Release

- [`.github/workflows/ci.yml`](.github/workflows/ci.yml)：push/PR → fmt、clippy、test、release build、Portal build  
- [`.github/workflows/release.yml`](.github/workflows/release.yml)：推送 `v*` tag → 多平台二进制 + Portal 资产发布到 GitHub Release  

```bash
git tag v0.1.0 && git push origin v0.1.0
```

## 环境变量

见 [`.env.example`](.env.example)。`PORT` 未设置时各二进制使用上表默认端口。

## 调用 API

```bash
curl http://127.0.0.1:3000/v1/chat/completions \
  -H "Authorization: Bearer sk-demo-playground" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hello"}]}'
```

令牌在用户中心创建。请求能转发之前，要在管理后台把渠道的上游 Key 换成真实密钥。上面的 `sk-demo-playground` 只在 `SEED_DEMO=1` 的数据库里存在。
