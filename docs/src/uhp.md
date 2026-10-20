# UHP 1.0 — Universal Harness Protocol

Term implements the Universal Harness Protocol (UHP) 1.0, a versioned
method registry for orchestrators and external automation. UHP runs over
the same Unix domain socket as the agent-native socket API
(`term --api`), but uses JSON-RPC 2.0 method calls.

## Protocol

UHP is a JSON-RPC 2.0 protocol. Clients send method call objects and
receive response objects.

### Method Call

```json
{
  "jsonrpc": "1.0",
  "method": "list_panes",
  "params": null,
  "id": 1
}
```

### Method Response

```json
{
  "jsonrpc": "1.0",
  "result": {...},
  "error": null,
  "id": 1
}
```

## Built-in Methods

### `list_panes`

List all panes with current state.

**Params:** none

**Result:**
```json
[
  {"id": 1, "title": "Claude Code #1", "agent": "claude", "status": "working", "cost_usd": 0.0},
  {"id": 2, "title": "Codex #2", "agent": "codex", "status": "blocked", "cost_usd": 1.25}
]
```

### `get_status`

Get full mission-control state.

**Params:** none

**Result:** Same as the Socket API `get_status` response.

## Events

UHP supports event subscription. Clients can subscribe to events and
receive event objects when they fire.

### Event Format

```json
{
  "event": "pane_spawn",
  "params": {"pane_id": 3, "agent": "claude"}
}
```

### Events

| Event | Description | Params |
|-------|-------------|--------|
| `pane_spawn` | A new pane was created | `{pane_id, agent}` |
| `pane_close` | A pane was closed | `{pane_id}` |
| `agent_blocked` | An agent became blocked | `{pane_id, agent}` |
| `agent_done` | An agent finished | `{pane_id, agent}` |
| `task_done` | A task was completed | `{task_id}` |
| `race_winner` | A race was won | `{race_id, winner}` |

## Example (Python)

```python
import socket, json

SOCK = "/home/user/.local/share/term/term.sock"

def uhp_call(method, params=None, id=1):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(SOCK)
    msg = {"jsonrpc": "1.0", "method": method, "params": params, "id": id}
    s.sendall(json.dumps(msg).encode())
    data = s.recv(65536)
    s.close()
    return json.loads(data.decode())

# List panes
print(uhp_call("list_panes"))

# Get status
print(uhp_call("get_status"))
```

## Version

The current protocol version is **1.0**, as reported by `UHP_VERSION`.
