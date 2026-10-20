# GitHub Release 包布局

每个平台一个主包：`danmo-router-<tag>-<target>.tar.gz`

解压后目录示例：

```text
danmo-router-v0.3.1-x86_64-unknown-linux-gnu/
  danmo-relay              # 二进制（与 v0.2 相同，位于根目录）
  danmo-admin
  danmo-user
  .env.example
  apps/admin/              # 管理 Portal 静态资源
  apps/user/               # 用户 Portal 静态资源
  deploy/                  # Nginx / systemd / setup-nginx.sh
  packages/                # 按角色分包（见 packages/README.md）
  RELEASE-LAYOUT.md        # 本说明
```

另附 **`danmo-portals-<tag>.tar.gz`**（仅 `admin/` + `user/` 静态），与旧版兼容；**推荐用主包** 一次拿齐二进制与 Portal。

首次发布 / 升级：

```bash
sudo ./deploy/scripts/install.sh --role=all --prefix=/opt/danmo
sudo ./deploy/scripts/upgrade.sh --from-dir=. --prefix=/opt/danmo --role=all
```
