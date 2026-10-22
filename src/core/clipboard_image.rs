//! Clipboard image support (from Luvus).
//!
//! Reads clipboard images (PNG) and injects them into the active pane's PTY
//! as base64-encoded data or saves to a file for agent consumption.
//! Uses wl-clipboard on Linux, osascript/pbpaste on macOS.

use std::io::Write;

/// Read clipboard image data (PNG bytes).
/// Returns `Ok(Vec<u8>)` with PNG data, or `Err` if no image available.
pub fn read_clipboard_image() -> Result<Vec<u8>, String> {
    #[cfg(target_os = "linux")]
    {
        // Try wl-clipboard first (Wayland), then xclip (X11).
        if let Ok(output) = std::process::Command::new("wl-paste")
            .args(["--type", "image/png"])
            .output()
        {
            if output.status.success() && !output.stdout.is_empty() {
                return Ok(output.stdout);
            }
        }
        if let Ok(output) = std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "image/png", "-o"])
            .output()
        {
            if output.status.success() && !output.stdout.is_empty() {
                return Ok(output.stdout);
            }
        }
        Err("no clipboard image found".to_string())
    }
    #[cfg(target_os = "macos")]
    {
        // macOS: use osascript to get clipboard as PNG.
        let script = "the clipboard as «class PNG»";
        let output = std::process::Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| format!("osascript: {e}"))?;
        if output.status.success() && !output.stdout.is_empty() {
            return Ok(output.stdout);
        }
        Err("no clipboard image found".to_string())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = ();
        Err("clipboard image not supported on this platform".to_string())
    }
}

/// Write clipboard image data to a file in the term data dir.
/// Returns the file path.
pub fn save_clipboard_image(data: &[u8]) -> Result<std::path::PathBuf, String> {
    let dir = crate::config::data_dir().join("clipboard");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("clipboard-{timestamp}.png"));
    let mut f = std::fs::File::create(&path).map_err(|e| format!("create file: {e}"))?;
    f.write_all(data).map_err(|e| format!("write: {e}"))?;
    Ok(path)
}

/// Paste clipboard image into the active pane's PTY.
/// On Linux with kitty graphics protocol support, this could use kitty
/// image escape sequences. For now, saves to file and reports path.
pub fn paste_clipboard_image(app: &mut crate::app::App) -> Result<(), String> {
    let data = read_clipboard_image()?;
    let path = save_clipboard_image(&data)?;
    let pane_idx = app.active_pane;
    if let Some(pane) = app.panes.get_mut(pane_idx) {
        if let Some(pty) = &mut pane.pty {
            // Notify the agent that an image was pasted (file path).
            let msg = format!(
                "\r\n[image pasted: {} ({} bytes)]\r\n",
                path.display(),
                data.len()
            );
            let _ = pty.write(msg.as_bytes());
            app.status_message = format!("image pasted: {} ({} bytes)", path.display(), data.len());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_clipboard_image_creates_file() {
        let data = b"fake-png-data";
        let path = save_clipboard_image(data).unwrap();
        assert!(path.exists());
        assert!(path.extension().unwrap() == "png");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
