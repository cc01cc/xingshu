# Architecture

Xingshu is a local project index and operation layer, not a hosted Git forge. Git is the first project backend. The `xingshu-core` Rust crate has no Web dependency and is shared by the CLI and Axum server.

```text
xingshu-core -> SQLite index and Git adapter
      |-> xingshu CLI
      |-> Axum server -> Vue WebUI
      `-> future Rust consumers
```

The server uses `127.0.0.1:12681`; Vite development uses `127.0.0.1:12680`. Scanner discovery is recursive, stops at repository boundaries by default, detects standard and bare repositories, and preserves user-set repository kinds on later scans.
