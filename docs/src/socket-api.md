# Socket API (`term --api`)

Term can run as a background socket server, accepting JSON commands from
external processes (agents, scripts, orchestrators). This is the same
agent-native model used by Herdr.

## Start the Server

```bash
term --api
```

This starts a Unix domain socket server at:

```
~/.local/share/term/term.sock
```

## Protocol

The server accepts one JSON command per connection and returns one JSON
response. All commands use JSON-RPC 2.0-style format.

### Commands

#### `list_panes`

List all panes with their current state.

**Request:**
```json
{"cmd": "ListPanes"}
```

**Response:**
```json
{
  "ok": "ok",
  "data": [
    {
      "id": 1,
      "title": "Claude Code #1",
      "agent": "claude",
      "status": "working",
      "cost_usd": 0.0
    },
    {
      "id": 2,
      "title": "Codex #2",
      "agent": "codex",
      "status": "blocked",
      "cost_usd": 1.25
    }
  ]
}
```

#### `get_status`

Get the full mission-control dashboard state as JSON.

**Request:**
```json
{"cmd": "GetStatus"}
```

**Response:**
```json
{
  "ok": "ok",
  "data": {
    "cwd": "/home/user/project",
    "panes": [
      {"id": 1, "title": "Claude Code #1", "agent": "claude", "status": "working", "cost_usd": 0.0, "attention": false, "session_id": null}
    ],
    "race": null,
    "queue": [],
    "inbox": {"unread": 0, "total": 0},
    "tasks": {"pending": 0, "running": 0, "blocked": 0, "done": 0, "ready": []},
    "modules": 0,
    "race_history": 0
  }
}
```

#### `spawn_agent`

Spawn an agent in a new pane.

**Request:**
```json
{
  "cmd": "SpawnAgent",
  "agent": "claude",
  "cwd": "/home/user/project"
}
```

**Response:**
```json
{
  "ok": "ok",
  "data": {
    "agent": "claude",
    "pane_id": 3
  }
}
```

#### `send_message`

Send text to a pane's PTY (as if typed).

**Request:**
```json
{
  "cmd": "SendMessage",
  "pane": 1,
  "message": "hello from socket API"
}
```

**Response:**
```json
{
  "ok": "ok",
  "data": {
    "pane": 1,
    "sent": 21
  }
}
```

#### `close_pane`

Close a pane by its ID.

**Request:**
```json
{"cmd": "ClosePane", "pane": 1}
```

**Response:**
```json
{
  "ok": "ok",
  "data": {"pane": 1, "closed": true}
}
```

## Example (Python)

```python
import socket, json

SOCK = "/home/user/.local/share/term/term.sock"

def send(cmd):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(SOCK)
    s.sendall(json.dumps(cmd).encode())
    data = s.recv(65536)
    s.close()
    return json.loads(data.decode())

# List all panes
print(send({"cmd": "ListPanes"}))

# Spawn an agent
print(send({"cmd": "SpawnAgent", "agent": "claude", "cwd": "/home/user/project"}))

# Send a message to pane 1
print(send({"cmd": "SendMessage", "pane": 1, "message": "hello"}))
```

## Example (Bash + curl)

```bash
# Note: curl doesn't support Unix domain sockets on all versions.
# Use socat or nc -U instead:
echo '{"cmd": "ListPanes"}' | socat - UNIX-CONNECT:~/.local/share/term/term.sock
```
