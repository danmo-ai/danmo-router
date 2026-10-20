# danmo-router

多协议 LLM 中转站：Chat、Responses、Messages、Gemini、Embeddings 转发，管理 Portal 和用户 Portal。客户端按模型看到的协议行为与原厂一致；渠道按每个模型配置上游方言。

- 设计：[docs/architecture.md](docs/architecture.md)
- 与 One API、New API、LiteLLM、GPT-Load、Sub2API 等的客观对比：[docs/comparison.md](docs/comparison.md)
- relay 渠道熔断与 429 降权（`CB_*` / `RL_*`）：[docs/channel-health.md](docs/channel-health.md)

## 架构

三个可独立部署的二进制（共享同一 `DATABASE_URL`）：

| 二进制 | 默认端口 | 职责 |
|--------|----------|------|
| `danmo-relay` | 3000 | 流量转发（可水平扩容） |
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

## 转发

入站协议由路径决定。模型价格上的 `native_dialects` 是原厂方言；渠道下每个模型一行，带上游模型名和上游方言。两端协议和方言相同则透传原字节，否则经中间表示转换。`opaque` 行只透传同协议请求。用量按响应字段计费，没有可用字段则退款并记 `billing_failed`。

| 路径 | 协议 |
|------|------|
| `POST /v1/chat/completions` | OpenAI Chat |
| `POST /v1/responses` | OpenAI Responses |
| `POST /v1/messages` | Anthropic Messages |
| `POST /v1/embeddings` | Embeddings |
| `POST /v1beta/models/{model}:generateContent` | Gemini |
| `POST /v1beta/models/{model}:streamGenerateContent` | Gemini 流式 |
| `GET /v1/models` | 模型列表 |

完整的方言列表、转发判定、数据模型和计费公式见 [系统架构](docs/architecture.md)。

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

`CB_*` / `RL_*` 等 relay 变量见 [渠道健康与选路](docs/channel-health.md#环境变量)。

## 调用 API

先在管理后台配置渠道和模型价格。令牌用 `Authorization: Bearer sk-...`；Messages 也接受 `x-api-key`，Gemini 也接受 `x-goog-api-key`。

```bash
curl http://127.0.0.1:3000/v1/chat/completions \
  -H "Authorization: Bearer sk-..." \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hello"}]}'

curl http://127.0.0.1:3000/v1/messages \
  -H "x-api-key: sk-..." \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet-4-5","max_tokens":256,"messages":[{"role":"user","content":"hello"}]}'
```
