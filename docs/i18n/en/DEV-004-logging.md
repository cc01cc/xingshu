# Logging and Audit

Operational logs diagnose processes; `fetch_log` records pull business history. They are separate layers.

Core emits `tracing` events but does not install a global subscriber. CLI and server install the subscriber at their entry points. `XINGSHU_LOG_LEVEL` takes precedence over `RUST_LOG`; stderr is the default sink, while `XINGSHU_LOG_FILE` enables daily/size-rotated JSONL file logging with a default 10 MiB file limit and seven retained rotations. `XINGSHU_LOG_MAX_BYTES` and `XINGSHU_LOG_MAX_FILES` override those defaults. Server events include `request_id` and `duration_ms`.

Structured fields include timestamp, level, operation, request ID, repository ID, root ID, duration, result, and error chain. Remote credentials, Authorization, Bearer, PAT, URL userinfo, and full environment variables must never appear in logs.
