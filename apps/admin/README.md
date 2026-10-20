# 管理后台

Danmo Router 的管理 Portal。开发服务器默认 `http://localhost:5173`，把 `/api` 代理到管理 API（默认 `http://127.0.0.1:3001`，可用 `VITE_API_PROXY` 覆盖）。

```bash
cd apps/admin
npm ci
npm run dev
```

在仓库根目录也可以 `make portals-dev-admin`。默认管理员是 `admin` / `admin123`（`ADMIN_USERNAME` / `ADMIN_PASSWORD`）。`SEED_DEMO=1` 时概览、渠道、用户、价格和日志会带上示例数据，截图见 [根 README](../../README.md#界面)。

```bash
npm run lint
npm run build
```
