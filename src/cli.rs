use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "ridge", version, about = "Mission control for your AI agents")]
pub struct Cli {
    /// Working directory to start in.
    #[arg(short, long)]
    pub cwd: Option<std::path::PathBuf>,

    /// Run in headless mode (no TUI) — for scripting/agents.
    #[arg(long)]
    pub headless: bool,

    /// Print the dashboard state as human-readable JSON and exit (no TUI).
    #[arg(long)]
    pub dashboard: bool,

    /// Run environment checks (agents on PATH, git/gh, data dir) and exit.
    #[arg(long)]
    pub doctor: bool,

    /// With --doctor, print copy-paste fix suggestions for failing checks.
    #[arg(long)]
    pub fix: bool,

    /// Restore panes from the previously saved session.
    #[arg(long)]
    pub restore: bool,

    /// Start the agent-native socket API server (Unix domain socket).
    #[arg(long)]
    pub api: bool,

    /// Check for and install updates from GitHub releases.
    #[arg(long)]
    pub update: bool,

    /// Start the UHP 1.0 agent-native socket server (Unix domain socket).
    #[arg(long)]
    pub uhp: bool,

    /// Start the Luvus Web browser client (HTTP + WebSocket).
    #[arg(long)]
    pub web: bool,

    /// Unix domain socket path to connect to a running `ridge --api` instance.
    /// When used with --web, the dashboard shows real panes from the server.
    #[arg(long)]
    pub socket: Option<std::path::PathBuf>,

    /// Start the background server (daemon mode) for detach/reattach.
    #[arg(long)]
    pub server: bool,

    /// Attach to a running background server as a TUI client.
    #[arg(long)]
    pub attach: bool,
}
