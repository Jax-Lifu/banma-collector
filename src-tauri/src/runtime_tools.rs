use std::path::PathBuf;
use tokio::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn command(name: &str) -> PathBuf {
    let executable = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if let Ok(current) = std::env::current_exe() {
        if let Some(directory) = current.parent() {
            let bundled = directory.join(&executable);
            if bundled.is_file() {
                return bundled;
            }
        }
    }
    PathBuf::from(executable)
}

/// Prevent command-line media tools from flashing a console window in the
/// packaged Windows application. This has no effect on other platforms.
pub(crate) fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_has_platform_executable_suffix() {
        let command = command("ffmpeg");
        assert!(command
            .to_string_lossy()
            .ends_with(&format!("ffmpeg{}", std::env::consts::EXE_SUFFIX)));
    }
}
