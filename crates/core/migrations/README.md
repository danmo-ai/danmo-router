# Schema migrations（按数据库类型）

| 目录 | 状态 |
|------|------|
| [`sqlite/`](sqlite/) | 已实现；单机默认引擎 |
| [`mysql/`](mysql/) | 已实现（MySQL 8 / MariaDB 10.5+）；多实例部署使用 |

两侧**同逻辑版本语义**，DDL 按方言单独维护。进程启动时由 `danmo_core::db::migrate` 按 `DATABASE_URL` 的 scheme 选择引擎并应用对应目录。

## 命名

`NNN_short_description.sql`，例如 `005_add_foo.sql`。序号在**该引擎目录内**递增；同一功能变更在 sqlite 与 mysql 两侧都必须有同号文件（方言可不同）。

## 版本记录

应用记录在库表 `schema_migrations(version)`。每个版本只执行一次。升级时只跑尚未登记的增量文件。多个进程同时启动时：SQLite 靠 `BEGIN IMMEDIATE` 串行；MySQL 靠 `GET_LOCK('danmo_migrate')` 串行（MySQL DDL 隐式提交，不能只靠事务）。

## 新增迁移检查清单

1. 在 `sqlite/` 新增 `NNN_….sql`（勿改旧文件）。
2. 在 `mysql/` 新增同号文件（MySQL 方言，见 [`mysql/README.md`](mysql/README.md)）。
3. 在 `crates/core/src/db/sqlite.rs` 与 `crates/core/src/db/mysql.rs` 的 `MIGRATIONS` 列表登记。
4. `cargo test -p danmo-core`；设置 `DANMO_TEST_MYSQL_URL=mysql://root:pass@127.0.0.1:3306` 再跑一次 MySQL（CI 的 `mysql` job 也会跑）。
