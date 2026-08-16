# Stage 14 — Agent API + CLI

**Status:** not started
**Depends on:** 06–13 as available; minimum 02, 03, 06
**Primary crates:** `splicecraft-agent`, `splicecraft-cli`, `splicecraft` binary

## Goal

Localhost JSON API (`--agent` / `--headless`) with `/tools` schema and
`splicecraft-cli call` passthrough, covering the workbench as it exists.
Parity with upstream's 230+ endpoints is the **end state**; land a stable
registry and the endpoints for every **already implemented** stage, then
fill the rest.

## Upstream (read before coding)

- `splicecraft_agent.py` — `_agent_endpoint`, handlers, write-guard
- `splicecraft_cli.py` — stdlib sidecar
- `docs/agent-api.md`, `docs/cli.md`
- `tests/test_agent_api.py`, `tests/test_cli_client.py`,
  `tests/test_new_agent_endpoints.py`

Suggested server: `axum` on `127.0.0.1` only.

## Rust targets

- `splicecraft --agent` (TUI + API) and `--headless` (`/healthz`)
- Handler registry with request JSON schema in `/tools`
- Write guard: dirty/unsaved policy matching upstream; 4xx on unauthorized
- Destructive whole-library wipe **not** reachable via agent
- Online search endpoints refuse unless the setting is on
- `splicecraft-cli call <endpoint> <json>`
- Path sanitiser for any file argument
- Bind localhost only; no `0.0.0.0`

## Sacred invariants

[INV-07] on writes. No sequence in logs. No silent egress.

## Acceptance

- [ ] `/healthz` 200 in headless
- [ ] `/tools` lists registered endpoints with schemas
- [ ] A read endpoint returns JSON for a sandboxed library
- [ ] A write endpoint without authorization fails
- [ ] CLI `call` hits the same registry
- [ ] Port bind test uses 127.0.0.1
- [ ] `cargo test -p splicecraft-agent -p splicecraft-cli`

## Forbidden

- Binding all interfaces
- Agent master-delete
- Shipping sequences to NCBI because an agent asked, while the setting is off

## Handoff

Stage 15 satellites: map image, BABS, OT-2, migrate, master delete, splice.
