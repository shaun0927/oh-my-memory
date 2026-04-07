# PROVIDERS

`oh-my-memory` supports **optional context providers**.

They are not required for the daemon to work.
They exist to improve accuracy when available.

## Design rules

- Providers are **optional**
- Providers are **advisory**
- Providers never replace the core policy engine
- Providers are queried **lazily** above their configured minimum pressure level

---

## tmux provider

The built-in tmux provider:

- checks whether tmux is available
- queries panes
- marks active pane PIDs as protected
- emits context notes describing active panes

It does **not** make tmux a hard dependency.

If tmux is unavailable, the core daemon still works.

---

## OpenChrome provider

The OpenChrome provider consumes JSON from an external command.

This keeps `oh-my-memory` decoupled from any specific OpenChrome transport while still allowing real context hints.

### Contract

The command must print JSON matching this schema:

```json
{
  "schema_version": 1,
  "source": "openchrome",
  "protected_pids": [111, 222],
  "stale_pids": [333],
  "notes": ["active browser session attached"],
  "active_workers": ["default"],
  "stale_workers": ["stale-1"]
}
```

### Field meanings

- `schema_version`: currently must be `1`
- `source`: provider source label, usually `openchrome`
- `protected_pids`: PIDs that must be protected
- `stale_pids`: PIDs that can receive an external stale bonus
- `notes`: human-readable context notes
- `active_workers`: optional worker ids that are actively in use
- `stale_workers`: optional worker ids believed to be stale

### Example config

```toml
[context.openchrome]
enabled = true
min_level = "orange"
command = "cat examples/openchrome-context.example.json"
```

### Example inspection

```bash
cargo run -- context providers --config config/oh-my-memory.example.toml --level orange
```

---

## Agent metadata provider

The agent provider is a generic external JSON provider for Codex/Claude-style session metadata.

It is useful when another tool can tell `oh-my-memory`:

- which PIDs are definitely tied to an active session
- which PIDs are stale helpers
- which PIDs should be treated as recently active even if CPU alone is ambiguous

### Contract

The command must print JSON matching this schema:

```json
{
  "schema_version": 1,
  "source": "agents",
  "protected_pids": [444],
  "stale_pids": [555],
  "recent_pids": [666],
  "notes": ["codex session is currently active"],
  "active_sessions": ["codex-main"],
  "idle_sessions": ["claude-idle-1"]
}
```

### Field meanings

- `schema_version`: currently must be `1`
- `source`: provider source label, usually `agents`
- `protected_pids`: PIDs that must be protected
- `stale_pids`: PIDs that can receive an external stale bonus
- `recent_pids`: PIDs that should be treated as recently active/protected
- `notes`: human-readable notes
- `active_sessions`: optional list of active agent/session identifiers
- `idle_sessions`: optional list of idle/stale agent/session identifiers

### Example config

```toml
[context.agents]
enabled = true
min_level = "orange"
command = "./examples/agent-provider.example.sh"
```

---

## Why providers are optional

`oh-my-memory` is intentionally process-first.

That means:

- the daemon can already detect many stale heavy processes generically
- providers only improve confidence
- missing providers should never block memory management

This keeps the system:

- lightweight
- portable
- explainable
- robust in mixed environments
