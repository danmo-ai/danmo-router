# Schema migrations（按数据库类型）

| 目录 | 状态 |
|------|------|
| [`sqlite/`](sqlite/) | 已实现；进程启动时由 `danmo_core::db::migrate` 应用 |
| [`mysql/`](mysql/) | 即将支持；与 sqlite **同逻辑版本语义**，DDL 按 MySQL 方言单独维护 |

## 命名

`NNN_short_description.sql`，例如 `005_add_foo.sql`。序号在**该引擎目录内**递增；同一功能变更在 sqlite 与 mysql 两侧都应有对应文件（方言可不同）。

## 版本记录

应用记录在库表 `schema_migrations(version)`。每个版本只执行一次。升级时只跑尚未登记的增量文件。

## 新增迁移检查清单

1. 在 `sqlite/` 新增 `NNN_….sql`（勿改旧文件）。
2. 在 `mysql/` 增加对等逻辑的 SQL（MySQL 方言）。
3. 更新 `crates/core/src/db.rs` 中对应引擎的 `include_str!` 列表。
4. `cargo test -p danmo-core`。
