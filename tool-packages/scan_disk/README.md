# scan_disk

`scan_disk` 是 iPet 的内置磁盘扫描工具，用于递归扫描指定目录并返回按大小排序的目录树摘要。

## 功能

- 扫描指定本地目录
- 统计文件数量、目录数量和总大小
- 按占用大小排序子节点
- 限制展示深度和每层子节点数量
- 缓存扫描结果到应用 SQLite 数据库

## 工具配置

工具配置见 `tool.json`。这是一个 `builtin` 工具，需要宿主应用提供 Rust 内置调度逻辑：

```json
{
  "name": "scan_disk",
  "kind": "builtin",
  "parameters": {
    "type": "object",
    "required": ["path"],
    "properties": {
      "path": { "type": "string" },
      "max_depth": { "type": "integer" },
      "max_children": { "type": "integer" }
    }
  }
}
```

## 源码

本包是一个独立 Cargo crate（`ipet-tool-scan-disk`），既是 iPet 运行时通过 path 依赖引入的真源，也可单独分发：

```text
rust/
├── Cargo.toml
└── src/
    ├── app_error.rs
    ├── disk_scanner.rs
    ├── lib.rs
    └── testutil.rs
```

核心入口：

- `rust/src/disk_scanner.rs`：目录递归扫描实现（取消、进度回调、软超时）
- `rust/src/app_error.rs`：独立包所需的最小错误类型
- `rust/src/lib.rs`：包导出与 `run_tool(path, max_depth, max_children)` 示例入口

依赖：`rayon`、`walkdir`、`serde`、`serde_json`、`chrono`、`thiserror`。

独立调用示例：

```rust
let json = ipet_tool_scan_disk::run_tool("C:\\Users", Some(4), Some(12))?;
```

## 参数

| 参数 | 类型 | 必填 | 默认 | 说明 |
|---|---|---:|---:|---|
| `path` | string | 是 | - | 要扫描的本地目录绝对路径。 |
| `max_depth` | integer | 否 | 4 | 递归展示深度，范围 1-12。 |
| `max_children` | integer | 否 | 12 | 每层最多返回的子节点数量，范围 1-64。 |

## 返回数据

返回 JSON 字符串，结构由 `DiskScanResult` 序列化而来，主要字段包括：

- `root`
- `scannedEntries`
- `elapsedMs`
- `truncated`
- `scannedAt`

`root` 是目录树节点，包含：

- `name`
- `path`
- `isDir`
- `sizeBytes`
- `fileCount`
- `dirCount`
- `children`

## 模型调用场景

适合在用户询问以下内容时调用：

- “帮我看看哪个目录占空间。”
- “扫描这个路径的磁盘占用。”
- “C 盘哪里最大？”
- “帮我找大文件夹。”

## 安全说明

- 本工具只读文件系统，不删除或修改文件。
- 不跟随符号链接。
- 大目录扫描可能耗时较长，应在 UI 中显示忙碌或计时状态。

## 接入说明

1. 将 `tool.json` 复制到工具注册表或导入流程。
2. iPet 已通过 workspace path 依赖直接使用 `rust/` crate；其他宿主可把 `rust/` 作为独立 crate 引入。
3. `openai-function.json` 由 `tool.json` 派生（`npm run sync:tools` 重新生成），将其内容加入 OpenAI 兼容接口的 `tools` 列表。
4. 宿主调度器收到 `scan_disk` 调用后执行 `scan_path(...)` 或 `run_tool(...)`。
