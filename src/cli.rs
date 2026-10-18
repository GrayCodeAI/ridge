use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "term", version, about = "Mission control for your AI agents")]
pub struct Cli {
    /// Working directory to start in.
    #[arg(short, long)]
    pub cwd: Option<std::path::PathBuf>,

    /// Run in headless mode (no TUI) — for scripting/agents.
    #[arg(long)]
    pub headless: bool,
}
