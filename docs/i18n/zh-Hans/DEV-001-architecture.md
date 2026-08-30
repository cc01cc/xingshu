# 星枢架构

## 定位

星枢不是远程 Git forge，而是本地项目索引与操作层。Git 仓库是第一种项目类型；核心设计保留未来接入其他资源类型的边界。

## 分层

```text
xingshu-core
  ├── index       SQLite schema, migrations, tags, policies, audit
  ├── scanner     multi-root recursive discovery
  ├── GitBackend  system git CLI adapter
  ├── puller      policy-aware pull and conflict decisions
  └── mover       cross-root relocation
        ├── xingshu CLI
        └── xingshu server (Axum) ─── webui (Vue)
```

`xingshu-core` 不依赖 Web 框架，可被后续 Rust 项目直接依赖。Server 默认监听 `127.0.0.1:12681`，Vite dev server 使用 `127.0.0.1:12680`。

## Scanner 边界

Scanner 递归查找 standard Git、bare Git 和可选的嵌套仓。遇到仓库边界默认停止；空目录、剪枝目录和生成的 `.bak`/`.broken` 目录不入库。repo kind 首次扫描启发赋值，后续扫描保留用户覆盖。
