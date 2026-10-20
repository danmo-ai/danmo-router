# 部署示例（首次发布 / 升级 / Nginx）

`danmo-relay` / `danmo-admin` / `danmo-user` 监听 **HTTP**（默认 3000/3001/3002）。公网 **TLS 在 Nginx 终结**。Schema 由进程启动时按引擎目录增量迁移（`crates/core/migrations/sqlite/`；**MySQL 即将支持** 见 `migrations/mysql/`）。

**发布包布局：** GitHub Release 主 tar 与 `make package` 的 **`dist/`** 结构一致（见 [RELEASE-LAYOUT.md](RELEASE-LAYOUT.md)）。**`dist/packages/danmo-user-portal/`** = 用户 API + Portal；**`danmo-admin-portal/`** = 管理 API + Portal；**`danmo-relay/`** = 仅转发。

## 推荐拓扑

```text
api.example.com:443   ──► Nginx ──► http://127.0.0.1:3000  (danmo-relay)
admin.example.com:443 ──► Nginx ──► 静态 Portal + /api ──► http://127.0.0.1:3001
user.example.com:443  ──► Nginx ──► 静态 Portal + /api ──► http://127.0.0.1:3002
```

三进程共用同一逻辑库（sqlite 现用；mysql 即将支持）。**不要**按角色拆三套库。

## 首次发布

```bash
# 解压 Release 主 tar 后，在包内或拷到 /opt/danmo 后：
sudo ./deploy/scripts/install.sh
# 分包机示例：
sudo ./deploy/scripts/install.sh --role=user --prefix=/opt/danmo-user
sudo ./deploy/scripts/install.sh --role=relay --prefix=/opt/danmo-relay --skip-nginx   # 或按需
```

`install.sh`：安装二进制/Portal、`.env`、systemd、Nginx，并启动服务。SQL **不由脚本执行**，进程启动时 migrate。

仅 Nginx：`sudo ./deploy/scripts/install.sh --nginx-only`  
（旧名 `setup-nginx.sh` 已转发到该入口。）

## 升级

```bash
# 解压新版本到例如 /tmp/danmo-vX
sudo ./deploy/scripts/upgrade.sh --from-dir=/tmp/danmo-vX --prefix=/opt/danmo --role=all
```

停服务 → 备份并替换二进制/Portal → 启动；**仅应用未登记的增量 SQL**（`schema_migrations`）。

## 文件说明

| 文件 | 用途 |
|------|------|
| `scripts/install.sh` | **首次发布** |
| `scripts/upgrade.sh` | **升级** |
| `scripts/setup-nginx.sh` | 兼容 → `install.sh --nginx-only` |
| `examples/nginx/*` | Nginx 模板 |
| `examples/systemd/*` | systemd 模板 |
| `packages/README.md` | 按角色分包说明 |

详细说明见 [docs/operations.md](../docs/operations.md) 与仓库根 [AGENTS.md](../AGENTS.md)。
