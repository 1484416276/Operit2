# Core Crate Layout

This directory is organized by domain first and by crate second.

Each first-level directory groups related crates. Each second-level directory is an actual Cargo crate with its own `Cargo.toml`, `src`, and `README.md`.

## Domains

- `foundation`: shared host contracts, models, utilities, and link protocol types.
- `persistence`: stores, repositories, identity data, and synchronization persistence.
- `provider`: LLM, media, market, memory, and local model services.
- `tool`: tool execution, builtin tools, skills, packages, MCP, and tool scripting.
- `plugin`: plugin SDK, SDK code generation, plugin runtime support, and JavaScript bridge.
- `runtime`: core business runtime and application-level services.
- `node`: node identity, pairing, authenticated peer links and their transports,
  route macros, node routing, space runtime and synchronization; also the compact
  Edge contracts and capability runtime used by constrained devices.
- `rslink`: pure Rust-to-link-to-Rust protocol runtime and shared code generation foundations.
- `proxy`: core-app proxy scan, language emitters, bridge contracts, local proxy
  runtime, and typed Edge Service projections.
- `application`: composition roots that assemble runtime, proxy, and node into one Core tree.
- `command`: command orchestration over the core application facade.

## Naming

Directory names describe the responsibility at that layer. Package names may retain public crate names during migration, but new physical paths should keep the domain name out of the second-level directory unless it adds meaning.
