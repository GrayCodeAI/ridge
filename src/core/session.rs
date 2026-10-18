use crate::core::agent::Agent;
use std::path::Path;

pub fn latest_session_file(agent: &dyn Agent, cwd: &Path) -> Option<std::path::PathBuf> {
    let mut files = agent.session_files(cwd);
    files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    files.into_iter().last()
}

pub fn resume_command(agent: &dyn Agent, cwd: &Path) -> Option<portable_pty::CommandBuilder> {
    if let Some(f) = latest_session_file(agent, cwd) {
        let stem = f.file_stem()?.to_string_lossy().to_string();
        if let Some(cmd) = agent.resume_cmd(&stem, cwd) {
            return Some(cmd);
        }
    }
    // Fall back to "last"
    agent.resume_cmd("last", cwd)
}
EOF