# MySQL migrations

**状态：已支持**（MySQL 8 / MariaDB 10.5+）。`DATABASE_URL=mysql://…` 时由 [`crates/core/src/db/mysql.rs`](../../src/db/mysql.rs) 的 `MIGRATIONS` 列表加载本目录。

与 [`../sqlite/`](../sqlite/) 同号同语义，方言独立编写。约定：

- 主键 `BIGINT AUTO_INCREMENT`；表尾统一 `ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin`（`_bin` 与 SQLite 的大小写敏感比较一致）。
- 文本列用 `VARCHAR(n)`，**不要用 `TEXT`/`BLOB`**：sqlx Any 驱动把它们映射为二进制，按 `String` 解码会失败。注意 InnoDB 单行 65535 字节上限，长字段（如 `logs.raw_usage`）在应用层截断。
- 时间列 `VARCHAR(32) NOT NULL`，无默认值；由 Rust 绑定 `db::now()`（`%Y-%m-%d %H:%M:%S` UTC），与 SQLite 存储格式一致。
- 不用 `TINYINT` / `DECIMAL`（Any 驱动不支持）：整数与布尔（0/1）一律 `BIGINT`，比率用 `DOUBLE`；`SUM` 结果包 `CAST(... AS SIGNED)`。
- 外键写成表级 `FOREIGN KEY (...) REFERENCES ...`（InnoDB 忽略列内联 `REFERENCES`）。
- 文件末尾语句不要求 `;`；按 `;` 切分执行，所以不要在字符串字面量里写 `;`。

共享查询里的方言差异（`RETURNING id`、`INSERT IGNORE`、upsert、`FOR UPDATE`、写事务开头）走 `SqlEngine` 钩子，不要在业务代码里判断引擎。
