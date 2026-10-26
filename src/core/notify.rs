//! OS notifications and sound alerts when agents need the user (blocked / finished).
//! Commands are literal argument vectors — no shell — so titles and bodies
//! can never inject anything.

use std::process::Command;

/// Build the notification command for the current platform.
/// Returns `(program, args)` or None on platforms without support.
pub fn command_for(title: &str, body: &str) -> Option<(String, Vec<String>)> {
    #[cfg(target_os = "macos")]
    {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            esc(body),
            esc(title)
        );
        Some(("osascript".to_string(), vec!["-e".to_string(), script]))
    }
    #[cfg(target_os = "linux")]
    {
        Some((
            "notify-send".to_string(),
            vec![
                "-a".to_string(),
                "ridge".to_string(),
                title.to_string(),
                body.to_string(),
            ],
        ))
    }
    #[cfg(target_os = "windows")]
    {
        let _ = (title, body);
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

/// Build the sound-alert command for the current platform.
/// Plays a short attention sound when an agent blocks or finishes.
/// Returns `(program, args)` or None on platforms without support.
pub fn sound_command() -> Option<(String, Vec<String>)> {
    #[cfg(target_os = "linux")]
    {
        // paplay is standard on PulseAudio/PipeWire systems.
        Some((
            "paplay".to_string(),
            vec!["/usr/share/sounds/freedesktop/stereo/complete.oga".to_string()],
        ))
    }
    #[cfg(target_os = "macos")]
    {
        // afplay plays the system default sound.
        Some((
            "afplay".to_string(),
            vec!["/System/Library/Sounds/Pop.aiff".to_string()],
        ))
    }
    #[cfg(target_os = "windows")]
    {
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

/// Fire-and-forget an OS notification. Never blocks on completion.
pub fn notify(title: &str, body: &str) -> bool {
    let Some((program, args)) = command_for(title, body) else {
        return false;
    };
    Command::new(&program).args(&args).spawn().is_ok()
}

/// Fire-and-forget a sound alert. Never blocks on completion.
pub fn sound() -> bool {
    let Some((program, args)) = sound_command() else {
        return false;
    };
    Command::new(&program).args(&args).spawn().is_ok()
}

/// Notify + sound together when an agent needs attention.
pub fn alert(title: &str, body: &str) {
    let _ = notify(title, body);
    let _ = sound();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_uses_notify_send_with_argument_vector() {
        let (prog, args) = command_for("Ridge — Claude #1", "agent is BLOCKED").unwrap();
        assert_eq!(prog, "notify-send");
        assert_eq!(args[1], "ridge");
        assert_eq!(args[2], "Ridge — Claude #1");
        // argument vectors: no shell, so quotes are literal-safe
        assert_eq!(args[3], "agent is BLOCKED");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_escapes_osascript_strings() {
        let body = r#"body with "quotes" and backslash \"#;
        let (prog, args) = command_for("t \"q\"", body).unwrap();
        assert_eq!(prog, "osascript");
        let script = &args[1];
        assert!(script.starts_with("display notification \""));
        assert!(script.ends_with("\""));
        // raw double quotes from the body never appear unescaped in the script
        assert!(!script.contains("body with \"quotes\""));
        assert!(script.contains("\\\"quotes\\\""));
        assert!(script.contains("t \\\"q\\\""));
    }

    #[test]
    fn notify_never_panics_on_unsupported_platforms() {
        // On unsupported platforms this is a safe no-op returning false;
        // on supported ones it spawns (result depends on the environment).
        let _ = notify("ridge test", "if you can read this in a toast, hooks work");
    }

    #[test]
    fn sound_never_panics_on_unsupported_platforms() {
        let _ = sound();
    }
}
