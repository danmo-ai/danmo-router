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

扩容建议：优先扩 `danmo-relay`；admin/user 通常少量副本即可。多副本写库时请使用支持并发的数据库（当前 SQLite WAL 适合轻量多进程，不适合大量 relay 副本）。

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

一体进程（调试用）：

```bash
make run-all-in-one
# Portal 需改代理：VITE_API_PROXY=http://127.0.0.1:3000 npm run dev
```

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
  -H "Authorization: Bearer sk-..." \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hello"}]}'
```

先在管理后台配置渠道与模型。
