//! Command line: `fsync-linux [--background] [--data-dir D] [--show] [--clip-send] [--clip-paste]
//! [--send <paths>...] [--version] [--build-info] [--help] [--smoke]`.

use std::path::PathBuf;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    pub background: bool,
    pub data_dir: Option<PathBuf>,
    pub show: bool,
    pub clip_send: bool,
    pub clip_paste: bool,
    pub send: Vec<PathBuf>,
    pub version: bool,
    pub build_info: bool,
    pub smoke: bool,
    pub help: bool,
}

impl Args {
    pub fn parse(raw: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut a = Args::default();
        let mut it = raw.into_iter().peekable();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--background" => a.background = true,
                "--show" => a.show = true,
                "--clip-send" => a.clip_send = true,
                "--clip-paste" => a.clip_paste = true,
                "--version" => a.version = true,
                "--build-info" => a.build_info = true,
                "--smoke" => a.smoke = true,
                "--help" | "-h" => a.help = true,
                "--data-dir" => {
                    let v = it
                        .next_if(|v| !v.starts_with("--"))
                        .ok_or("--data-dir needs a directory")?;
                    a.data_dir = Some(PathBuf::from(v));
                }
                "--send" => {
                    while let Some(p) = it.next_if(|p| !p.starts_with("--")) {
                        a.send.push(PathBuf::from(p));
                    }
                    if a.send.is_empty() {
                        return Err("--send needs at least one path".into());
                    }
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(v: &[&str]) -> Result<Args, String> {
        Args::parse(v.iter().map(|s| s.to_string()))
    }

    #[test]
    fn flags_and_values() {
        let a = parse(&["--background", "--data-dir", "/d", "--show"]).unwrap();
        assert!(a.background && a.show);
        assert_eq!(a.data_dir, Some(PathBuf::from("/d")));
    }

    #[test]
    fn send_takes_paths_until_next_flag() {
        let a = parse(&["--send", "/a", "/b", "--background"]).unwrap();
        assert_eq!(a.send, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
        assert!(a.background);
    }

    #[test]
    fn help_flag_is_known() {
        assert!(parse(&["--help"]).unwrap().help);
        assert!(parse(&["-h"]).unwrap().help);
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(parse(&["--send"]).is_err());
        assert!(parse(&["--data-dir"]).is_err());
        assert!(parse(&["--data-dir", "--background"]).is_err());
        assert!(parse(&["--nope"]).is_err());
        assert_eq!(parse(&[]).unwrap(), Args::default());
    }
}
