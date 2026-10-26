use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Manual,
    Idle,
    Smoke,
}

#[derive(Clone)]
pub struct Options {
    pub shell: String,
    pub sessions: u64,
    pub mode: Mode,
    pub output: PathBuf,
}

impl Options {
    pub fn parse() -> io::Result<Self> {
        let mut options = Self {
            shell: "cmd".into(),
            sessions: 2,
            mode: Mode::Manual,
            output: "target/compatibility/manual".into(),
        };
        let mut args = std::env::args().skip(1);
        while let Some(key) = args.next() {
            let value = args.next().ok_or_else(|| io::Error::other("expected --shell cmd|powershell|pwsh --sessions 1|2 --mode manual|idle|smoke --output DIRECTORY"))?;
            match key.as_str() {
                "--shell" if ["cmd", "powershell", "pwsh"].contains(&value.as_str()) => {
                    options.shell = value
                }
                "--sessions" if value == "1" || value == "2" => {
                    options.sessions = value.parse().map_err(io::Error::other)?
                }
                "--mode" => {
                    options.mode = match value.as_str() {
                        "manual" => Mode::Manual,
                        "idle" => Mode::Idle,
                        "smoke" => Mode::Smoke,
                        _ => return Err(io::Error::other("invalid mode")),
                    }
                }
                "--output" => options.output = value.into(),
                _ => return Err(io::Error::other("invalid compatibility harness arguments")),
            }
        }
        Ok(options)
    }
}

pub fn launch(shell: &str, fixture: &Path, id: u64) -> io::Result<(String, Vec<String>)> {
    let path = fixture
        .to_str()
        .ok_or_else(|| io::Error::other("fixture path must be Unicode"))?;
    // These are shell scripts, with cmd and PowerShell syntax kept separate.
    if path.contains(['%', '"', '\r', '\n']) {
        return Err(io::Error::other(
            "unsupported character in controlled fixture path",
        ));
    }
    if shell == "cmd" {
        Ok((
            "cmd.exe".into(),
            vec![
                "/D".into(),
                "/Q".into(),
                "/V:OFF".into(),
                "/S".into(),
                "/C".into(),
                format!("\"\"{path}\" {id}\""),
            ],
        ))
    } else {
        Ok((
            format!("{shell}.exe"),
            vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                format!("& '{}' {id}; exit $LASTEXITCODE", path.replace('\'', "''")),
            ],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controlled_shell_launch_preserves_spaces_and_powershell_apostrophes() {
        let path = Path::new("C:/fixture folder/it's-child.exe");
        let (_, args) = launch("powershell", path, 2).unwrap();
        assert_eq!(
            args.last().unwrap(),
            "& 'C:/fixture folder/it''s-child.exe' 2; exit $LASTEXITCODE"
        );
        let (_, args) = launch("cmd", Path::new("C:/fixture folder/child.exe"), 1).unwrap();
        assert_eq!(
            args.last().unwrap(),
            "\"\"C:/fixture folder/child.exe\" 1\""
        );
        assert!(launch("cmd", Path::new("C:/%PATH%/child.exe"), 1).is_err());
    }
}
