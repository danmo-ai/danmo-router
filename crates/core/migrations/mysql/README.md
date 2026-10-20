# MySQL migrations

**状态：即将支持。** 当前运行时仅加载 [`../sqlite/`](../sqlite/)；`DATABASE_URL=mysql://…` 会在连接阶段被拒绝。

落地 MySQL 时：

1. 为本目录补齐与 sqlite 同语义的 `001`…`NNN`（方言独立：`AUTO_INCREMENT`、`DATETIME`、索引语法等）。
2. 在 `db.rs` 增加 MySQL 连接与 `MYSQL_MIGRATIONS` 列表。
3. 保持 `schema_migrations.version` 与逻辑变更对齐，便于运维理解「已升到第几版」。

在驱动接通前，schema 变更仍须先完成 **sqlite** 侧文件与测试；本目录可同步预写 DDL。
