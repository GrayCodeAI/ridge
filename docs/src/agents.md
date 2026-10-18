# Agent Adapters

Term uses an `Agent` trait to support multiple coding agents.

## Implementing a new adapter

Create `src/agents/<name>.rs` implementing `Agent`:

```rust
use crate::core::agent::{Agent, AgentStatus, CostInfo};
use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};

pub struct MyAgent;

impl Agent for MyAgent {
    fn id(&self) -> &'static str { "myagent" }
    fn name(&self) -> &'static str { "My Agent" }
    fn detect(&self, process_name: &str, cmdline: &str) -> bool { ... }
    fn spawn_cmd(&self, cwd: &Path) -> CommandBuilder { ... }
    fn resume_cmd(&self, session_id: &str, cwd: &Path) -> Option<CommandBuilder> { None }
    fn session_files(&self, cwd: &Path) -> Vec<PathBuf> { Vec::new() }
    fn parse_status(&self, output_tail: &str) -> AgentStatus { ... }
    fn parse_cost(&self, session_file: &Path) -> Option<CostInfo> { None }
}
```

Register in `src/core/agent.rs` `AgentRegistry::new()`.

## Status detection
Parse the last few lines of terminal output. Return `Blocked` for permission prompts, `Idle` for shell prompts, `Working` for spinners.
