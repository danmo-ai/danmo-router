# 管理后台

Danmo Router 的管理 Portal。开发服务器默认 `http://localhost:5173`，把 `/api` 代理到管理 API（默认 `http://127.0.0.1:3001`，可用 `VITE_API_PROXY` 覆盖）。

```bash
cd apps/admin
npm ci
npm run dev
```

在仓库根目录也可以 `make portals-dev-admin`。默认管理员是 `admin` / `admin123`（`ADMIN_USERNAME` / `ADMIN_PASSWORD`）。

`SEED_DEMO=1` 时各页会带上示例数据：概览、渠道、用户、模型价格、兑换码、调用日志（含筛选与分页）。截图见 [根 README](../../README.md#界面)。

页面路由：`/` 概览，`/channels` 渠道，`/users` 用户（额度调整、重置密码、管理令牌），`/prices` 模型价格，`/redemptions` 兑换码批量生成，`/logs` 调用日志。

```bash
npm run lint
npm run build
```
