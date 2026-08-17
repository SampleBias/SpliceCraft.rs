# Stage 14 — Agent API + CLI

**Status:** done
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

- [x] `/healthz` 200 in headless
- [x] `/tools` lists registered endpoints with schemas
- [x] A read endpoint returns JSON for a sandboxed library
- [x] A write endpoint without authorization fails
- [x] CLI `call` hits the same registry
- [x] Port bind test uses 127.0.0.1
- [x] `cargo test -p splicecraft-agent -p splicecraft-cli`

## Notes

- Bind constant is `127.0.0.1` (`splicecraft_agent::BIND_HOST`). Host
  headers that are not loopback are 403 (DNS-rebinding defence).
- Registry: `splicecraft_agent::builtin()` — first-wave endpoints for
  stages 01–13 (library, ORFs, restriction, local BLAST, gated online
  search, settings, HMM catalog, experiments/primers/gels, PCR, file
  load/export). `/tools` includes `schema` plus `doc` / `doc_full`.
- Writes: dirty-guard 409 unless `{"force": true}` in the POST body;
  persist writes 403 unless `writes_authorized()`. Token-less HTTP is
  401. `set-setting` cannot enable `allow_online_search`.
- Token file: `<data-dir>/agent_token` (`port\\ntoken`), mode 0600.
  CLI refuses a symlink, a file > 1 KB, and the Python `splicecraft/`
  leaf. Response cap 50 MB.
- `splicecraft --agent` / `--headless` / `--agent-port`. `--headless`
  (or `SPLICECRAFT_HEADLESS=1`, or `--agent` with no TTY) is API-only.

## Forbidden

- Binding all interfaces
- Agent master-delete
- Shipping sequences to NCBI because an agent asked, while the setting is off

## Handoff

Stage 15 satellites: map image, BABS, OT-2, migrate, master delete, splice.
