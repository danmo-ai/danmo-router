# Agent / contributor rules (danmo-router)

## Database migrations（按引擎分目录）

三进程（relay / admin / user）**共用同一逻辑库**；迁移按 **数据库类型（引擎）** 分目录，不按角色拆库：

```text
crates/core/migrations/
  sqlite/     # 当前已实现
  mysql/      # 即将支持；与 sqlite 同序号语义，方言独立编写
  README.md
```

### 硬规则

1. **每次 schema 变更必须新增** `NNN_snake_case.sql`（三位序号递增）。**禁止修改**已合入、可能已被生产应用的旧迁移文件。
2. 当前引擎为 **SQLite**：新增文件放在 `migrations/sqlite/`，并在 [`crates/core/src/db.rs`](crates/core/src/db.rs) 的 `SQLITE_MIGRATIONS` 列表中登记（顺序 = 版本号）。
3. **MySQL（进行中）**：同一逻辑变更在 `migrations/mysql/` 增加**同号或同语义**的 SQL（方言可不同：`AUTO_INCREMENT`、类型、引号等），待 `mysql://` 连接接通后由对应列表加载。在 MySQL 未接通前，仍须为 sqlite 写完并跑通测试；mysql 侧可先落文件。
4. 勿在 shell 部署脚本里手写「全量建表」绕过 `schema_migrations`。增量只由进程启动时的 `migrate()` 应用。
5. 改迁移后运行相关测试（至少 `cargo test -p danmo-core`）。

### 不要做的事

- 不要为 relay / admin / user 各维护一套迁移目录（它们共享表结构）。
- 不要把 sqlite 与 mysql 的 DDL 混在同一文件里。

## 部署（Docker Compose / GHCR）

| 脚本 / 文件 | 用途 |
|-------------|------|
| [`compose.yml`](compose.yml) | 四服务：relay / admin / user / edge（仅 edge 映射宿主机端口） |
| [`deploy/scripts/install.sh`](deploy/scripts/install.sh) | **首次发布**：`.env` + `docker compose up` |
| [`deploy/scripts/upgrade.sh`](deploy/scripts/upgrade.sh) | **升级**：pull 或 build 后 recreate（SQL 增量由进程 migrate） |
| [`Dockerfile`](Dockerfile) | 多 target 镜像；Release 推送 GHCR |

角色用 `--role=all|relay|admin|user`（Compose **profiles**），**不要**恢复 systemd 或 `/opt/danmo` 主机安装链。

Portal 静态打进 `danmo-edge`，**不走 OSS/CDN**。改部署行为时同步更新 [`docs/operations.md`](docs/operations.md) 与 [`deploy/README.md`](deploy/README.md)。

开发与单测用 `make check` / `make run-*`；整栈冒烟用 Compose。

## 文档地图

- 系统设计：[`docs/architecture.md`](docs/architecture.md)
- 运维手册：[`docs/operations.md`](docs/operations.md)
- 合规：[`docs/compliance.md`](docs/compliance.md)
