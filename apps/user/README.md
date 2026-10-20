# 用户中心

Danmo Router 的用户 Portal。开发服务器默认 `http://localhost:5174`，把 `/api` 代理到用户 API（默认 `http://127.0.0.1:3002`，可用 `VITE_API_PROXY` 覆盖）。

```bash
cd apps/user
npm ci
npm run dev
```

在仓库根目录也可以 `make portals-dev-user`。页面可以注册，也可以用 `SEED_DEMO=1` 生成的 `demo` / `demo1234` 登录。

登录后可查看账户概览（额度、兑换码充值）、管理 API 令牌、浏览可用模型与价格、分页查看自己的调用日志。Relay 基址来自 `PUBLIC_RELAY_URL`（未设置时显示本地默认）。截图见 [根 README](../../README.md#界面)。

页面路由：`/` 账户，`/tokens` 令牌，`/models` 可用模型，`/logs` 调用日志。

```bash
npm run lint
npm run build
```
