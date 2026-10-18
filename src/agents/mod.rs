pub mod aider;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod gemini;
pub mod generic;
pub mod grok;
pub mod kimi;
pub mod opencode;
pub mod pi;

#[cfg(test)]
mod detect_tests {
    use crate::core::agent::Agent;

    fn matched<F>(f: &F, p: &str, c: &str) -> bool
    where
        F: Fn(&str, &str) -> bool,
    {
        f(p, c)
    }

    #[test]
    fn adapters_match_their_process_names() {
        use crate::agents::{
            aider::AiderAgent, claude::ClaudeAgent, codex::CodexAgent, copilot::CopilotAgent,
            cursor::CursorAgent, gemini::GeminiAgent, grok::GrokAgent, kimi::KimiAgent,
            opencode::OpenCodeAgent, pi::PiAgent,
        };

        // (detect function, matching process name, matching cmdline)
        let cases: Vec<(Box<dyn Fn(&str, &str) -> bool>, &str, &str)> = vec![
            (
                Box::new(|p, c| ClaudeAgent.detect(p, c)),
                "claude",
                "claude --resume",
            ),
            (
                Box::new(|p, c| CodexAgent.detect(p, c)),
                "codex",
                "codex exec",
            ),
            (
                Box::new(|p, c| OpenCodeAgent.detect(p, c)),
                "opencode",
                "opencode run",
            ),
            (
                Box::new(|p, c| GeminiAgent.detect(p, c)),
                "gemini",
                "gemini chat",
            ),
            (
                Box::new(|p, c| CopilotAgent.detect(p, c)),
                "copilot",
                "gh copilot suggest",
            ),
            (
                Box::new(|p, c| AiderAgent.detect(p, c)),
                "aider",
                "aider --model x",
            ),
            (
                Box::new(|p, c| KimiAgent.detect(p, c)),
                "kimi",
                "kimi --session 1",
            ),
            (Box::new(|p, c| PiAgent.detect(p, c)), "pi", "pi"),
            (
                Box::new(|p, c| GrokAgent.detect(p, c)),
                "grok",
                "grok build",
            ),
            (
                Box::new(|p, c| CursorAgent.detect(p, c)),
                "cursor-agent",
                "cursor-agent -p x",
            ),
        ];
        for (f, proc_name, cmdline) in &cases {
            assert!(
                f(proc_name, cmdline),
                "must detect its own process: {proc_name}:{cmdline}"
            );
        }
    }

    #[test]
    fn registry_returns_all_adapters() {
        let r = crate::core::agent::AgentRegistry::new();
        assert_eq!(r.all().len(), 11);
        assert_eq!(r.all()[10].id(), "generic"); // generic always last (fallback)
    }
}
