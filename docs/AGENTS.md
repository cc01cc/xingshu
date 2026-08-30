# Xingshu Documentation Index

This directory contains documentation specific to the Xingshu project. The workspace-level index is `one/docs/AGENTS.md`; this file is the project-level entry point.

## Canonical Documents

| Topic | Document |
|------|----------|
| Architecture | `i18n/zh-Hans/DEV-001-architecture.md` |
| CLI and configuration | `i18n/zh-Hans/DEV-002-cli-config.md` |
| Database schema | `i18n/zh-Hans/DEV-003-database-schema.md` |
| Logging and audit | `i18n/zh-Hans/DEV-004-logging.md` |
| Development and testing | `i18n/zh-Hans/DEV-005-development-testing.md` |
| WebUI usage | `i18n/zh-Hans/USER-001-webui.md` |
| HTTP API | `api/openapi.yaml` |

English documents are maintained in the matching `i18n/en/` paths. Each language document must state its synchronization status when content differs.

## Writing Rules

- Explain the Xingshu name: stars represent independent projects or resources; the hub gathers and manages them. Git repositories are the first supported type, not the only future type.
- Do not publish machine-specific credentials, tokens, private absolute paths, staging databases, or raw Playwright tool state.
- Commands that write repositories must use synthetic tempdir or a new staging run.
- API behavior is canonical in `api/openapi.yaml`; update it with route or response changes.
