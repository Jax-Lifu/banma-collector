use std::path::PathBuf;

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
