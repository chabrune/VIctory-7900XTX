//! Where the user's files live.
//!
//! Resolution order for each value: environment variable, then the config
//! file (`VICTORY_CONFIG`, else `./victory.toml`), then the default.
//! See `victory.example.toml` at the repository root.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::display::{Display, Mode};
use crate::input::{self, Bindings};
use crate::policy::ensure_outside_repo;

#[derive(Debug, Clone)]
pub struct Config {
    /// The user's dump of the title.
    pub game_dir: Option<PathBuf>,
    /// Folder containing the PC FidelityFX upscaler for the optional FSR profile.
    pub fidelityfx_dir: Option<PathBuf>,
    /// Root for all derived data. Always outside the repository.
    pub cache_dir: PathBuf,
    /// What titles write and must keep (persistent local storage). Never
    /// inside the cache, which may be deleted at any time.
    pub data_dir: PathBuf,
    /// Other titles used to exercise platform-level code.
    pub test_titles: Vec<PathBuf>,
    /// How the picture reaches the screen.
    pub display: Display,
    /// The keyboard bindings that stand in for a gamepad.
    pub input: Bindings,
    /// Where logs are also written; `None` keeps them on the console only.
    pub log_file: Option<PathBuf>,
    /// The config file that was read, if any.
    pub source: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            game_dir: None,
            fidelityfx_dir: None,
            cache_dir: PathBuf::new(),
            data_dir: PathBuf::new(),
            test_titles: Vec::new(),
            display: Display::default(),
            input: Bindings::default(),
            log_file: None,
            source: None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    paths: FilePaths,
    #[serde(default)]
    display: FileDisplay,
    #[serde(default)]
    log: FileLog,
    #[serde(default)]
    input: BTreeMap<String, String>,
}

/// `[display]`: the picture on the screen. Every field is optional, and
/// the environment overrides each one.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDisplay {
    mode: Option<String>,
    monitor: Option<usize>,
    vsync: Option<bool>,
    frame_limit: Option<u32>,
}

/// `[log]`: where the log file goes (`"off"` for none).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileLog {
    file: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePaths {
    game_dir: Option<PathBuf>,
    fidelityfx_dir: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    #[serde(default)]
    test_titles: Vec<PathBuf>,
}

impl Config {
    /// Loads configuration from the environment and the config file.
    pub fn load() -> Result<Self> {
        let explicit = explicit_config_path(|k| std::env::var_os(k));
        let path = explicit
            .clone()
            .unwrap_or_else(|| PathBuf::from("victory.toml"));
        let file = if path.is_file() {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            Some((path, text))
        } else if let Some(p) = explicit {
            anyhow::bail!(
                "VICTORY_CONFIG points at {}, which does not exist",
                p.display()
            );
        } else {
            None
        };
        let (source, parsed) = match file {
            Some((path, text)) => {
                let parsed: FileConfig =
                    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
                (Some(path), parsed)
            }
            None => (None, FileConfig::default()),
        };
        Self::resolve(parsed, source, |k| std::env::var_os(k))
    }

    fn resolve(
        file: FileConfig,
        source: Option<PathBuf>,
        env: impl Fn(&str) -> Option<std::ffi::OsString>,
    ) -> Result<Self> {
        let env_path = |k: &str| env(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let env_text = |k: &str| {
            env(k)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string_lossy().into_owned())
        };
        let from = source
            .as_ref()
            .map_or_else(|| "victory.toml".to_owned(), |p| p.display().to_string());
        let FilePaths {
            game_dir,
            fidelityfx_dir,
            cache_dir: file_cache,
            data_dir: file_data,
            test_titles: file_titles,
        } = file.paths;
        let test_titles = match env("VICTORY_TEST_TITLES") {
            Some(v) if !v.is_empty() => std::env::split_paths(&v)
                .filter(|p| !p.as_os_str().is_empty())
                .collect(),
            _ => file_titles,
        };
        let cache_dir = env_path("VICTORY_CACHE_DIR")
            .or(file_cache)
            .map_or_else(|| default_dir("cache"), Ok)?;
        let data_dir = env_path("VICTORY_DATA_DIR")
            .or(file_data)
            .map_or_else(|| default_dir("data"), Ok)?;

        let mode = if let Some(text) = env_text("VICTORY_WINDOW_MODE") {
            Mode::parse(&text).with_context(|| {
                format!("VICTORY_WINDOW_MODE={text:?}: use windowed, borderless or fullscreen")
            })?
        } else if let Some(text) = file.display.mode {
            Mode::parse(&text).with_context(|| {
                format!("{from}: [display] mode = {text:?}: use windowed, borderless or fullscreen")
            })?
        } else {
            Mode::default()
        };
        let monitor = match env_text("VICTORY_MONITOR") {
            Some(text) => text
                .parse()
                .with_context(|| format!("VICTORY_MONITOR={text:?}: a screen number, 0 first"))?,
            None => file.display.monitor.unwrap_or(0),
        };
        let vsync = match env_text("VICTORY_VSYNC") {
            Some(text) => boolean(&text)
                .with_context(|| format!("VICTORY_VSYNC={text:?}: use 1 or 0"))?,
            None => file.display.vsync.unwrap_or(false),
        };
        let frame_limit = match env_text("VICTORY_FPS_LIMIT") {
            Some(text) => text
                .parse::<u32>()
                .ok()
                .with_context(|| format!("VICTORY_FPS_LIMIT={text:?}: frames a second, 0 for no cap"))?
                .into(),
            None => file.display.frame_limit,
        }
        .filter(|&limit| limit > 0);
        let display = Display {
            mode,
            monitor,
            vsync,
            frame_limit,
        };

        let input = input::Bindings::from_table(&file.input)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("{from}: [input]"))?;

        // Logs: on by default, beside the cache; `off` turns the file off.
        let default_log = cache_dir.join("logs").join("victory.log");
        let log_file = match env_text("VICTORY_LOG_FILE") {
            Some(text) if text.eq_ignore_ascii_case("off") => None,
            Some(text) => Some(PathBuf::from(text)),
            None => match file.log.file.as_deref().map(str::trim) {
                Some("off") => None,
                Some(path) if !path.is_empty() => Some(PathBuf::from(path)),
                _ => Some(default_log),
            },
        };

        Ok(Config {
            game_dir: env_path("VICTORY_GAME_DIR").or(game_dir),
            fidelityfx_dir: env_path("VICTORY_FIDELITYFX_DIR").or(fidelityfx_dir),
            cache_dir,
            data_dir,
            test_titles,
            display,
            input,
            log_file,
            source,
        })
    }

    /// The game directory, or a helpful error if it is not configured.
    pub fn require_game_dir(&self) -> Result<&Path> {
        self.game_dir.as_deref().context(
            "no game directory configured: set VICTORY_GAME_DIR or [paths].game_dir in \
             victory.toml (see victory.example.toml)",
        )
    }

    /// Returns `<cache>/<sub>`, creating it, after checking the asset policy.
    pub fn cache_subdir(&self, sub: &str) -> Result<PathBuf> {
        subdir(&self.cache_dir, sub)
    }

    /// Returns `<data>/<sub>`, creating it, after checking the asset policy.
    pub fn data_subdir(&self, sub: &str) -> Result<PathBuf> {
        subdir(&self.data_dir, sub)
    }
}

/// The config file explicitly named by the environment, if any. An empty
/// `VICTORY_CONFIG` means unset, like the other path variables.
fn explicit_config_path(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    env("VICTORY_CONFIG")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// `1`/`0`, `true`/`false`, `on`/`off`, `yes`/`no`; nothing else.
fn boolean(text: &str) -> Result<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Ok(true),
        "0" | "false" | "off" | "no" => Ok(false),
        _ => anyhow::bail!("{text:?} is not a yes or a no"),
    }
}

fn subdir(root: &Path, sub: &str) -> Result<PathBuf> {
    let dir = root.join(sub);
    ensure_outside_repo(&dir)?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

/// `%LOCALAPPDATA%/victory/<name>` (elsewhere the XDG or home cache).
fn default_dir(name: &str) -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_CACHE_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .context("cannot determine a user directory; set VICTORY_CACHE_DIR and VICTORY_DATA_DIR")?;
    Ok(base.join("victory").join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn paths(paths: FilePaths) -> FileConfig {
        FileConfig {
            paths,
            ..Default::default()
        }
    }

    #[test]
    fn environment_overrides_file() {
        let file = paths(FilePaths {
            game_dir: Some("from-file".into()),
            cache_dir: Some("cache-from-file".into()),
            ..Default::default()
        });
        let env = |k: &str| (k == "VICTORY_GAME_DIR").then(|| OsString::from("from-env"));
        let cfg = Config::resolve(file, None, env).unwrap();
        assert_eq!(cfg.game_dir.unwrap(), PathBuf::from("from-env"));
        assert_eq!(cfg.cache_dir, PathBuf::from("cache-from-file"));
    }

    #[test]
    fn empty_env_values_are_ignored() {
        let file = paths(FilePaths {
            game_dir: Some("from-file".into()),
            ..Default::default()
        });
        let env = |k: &str| (k == "VICTORY_GAME_DIR").then(OsString::new);
        let cfg = Config::resolve(file, None, env).unwrap();
        assert_eq!(cfg.game_dir.unwrap(), PathBuf::from("from-file"));
    }

    #[test]
    fn empty_test_titles_env_is_ignored() {
        let file = paths(FilePaths {
            test_titles: vec!["from-file".into()],
            ..Default::default()
        });
        let env = |k: &str| (k == "VICTORY_TEST_TITLES").then(OsString::new);
        let cfg = Config::resolve(file, None, env).unwrap();
        assert_eq!(cfg.test_titles, vec![PathBuf::from("from-file")]);
    }

    #[test]
    fn test_titles_env_splits_paths() {
        let value = std::env::join_paths(["first", "second"]).unwrap();
        let file = paths(FilePaths::default());
        let env = |k: &str| (k == "VICTORY_TEST_TITLES").then(|| value.clone());
        let cfg = Config::resolve(file, None, env).unwrap();
        assert_eq!(
            cfg.test_titles,
            vec![PathBuf::from("first"), PathBuf::from("second")]
        );
    }

    #[test]
    fn empty_config_env_is_ignored() {
        let empty = |k: &str| (k == "VICTORY_CONFIG").then(OsString::new);
        assert_eq!(explicit_config_path(empty), None);
        let set = |k: &str| (k == "VICTORY_CONFIG").then(|| OsString::from("custom.toml"));
        assert_eq!(
            explicit_config_path(set),
            Some(PathBuf::from("custom.toml"))
        );
    }

    #[test]
    fn example_config_parses() {
        let text = include_str!("../../../victory.example.toml");
        let parsed: FileConfig = toml::from_str(text).unwrap();
        assert!(parsed.paths.game_dir.is_some());
    }

    #[test]
    fn display_and_input_come_from_the_file_and_the_environment_wins() {
        let text = "\n[display]\nmode = 'borderless'\nmonitor = 1\nvsync = true\n\
                    frame_limit = 60\n\n[input]\nls_up = 'z'\n\n[log]\nfile = 'off'\n";
        let parsed: FileConfig = toml::from_str(text).unwrap();
        let cfg = Config::resolve(parsed, None, |_| None).unwrap();
        assert_eq!(cfg.display.mode, Mode::Borderless);
        assert_eq!(cfg.display.monitor, 1);
        assert!(cfg.display.vsync);
        assert_eq!(cfg.display.frame_limit, Some(60));
        assert_eq!(cfg.input.key(crate::input::Action::LeftUp), Some(b'Z'));
        assert_eq!(cfg.log_file, None, "[log] file = 'off'");

        let parsed: FileConfig = toml::from_str(text).unwrap();
        let env = |k: &str| match k {
            "VICTORY_WINDOW_MODE" => Some(OsString::from("windowed")),
            "VICTORY_VSYNC" => Some(OsString::from("0")),
            "VICTORY_FPS_LIMIT" => Some(OsString::from("0")),
            _ => None,
        };
        let cfg = Config::resolve(parsed, None, env).unwrap();
        assert_eq!(cfg.display.mode, Mode::Windowed);
        assert!(!cfg.display.vsync);
        assert_eq!(cfg.display.frame_limit, None, "0 is no cap");
    }

    #[test]
    fn the_log_file_defaults_to_the_cache_and_bad_values_are_named() {
        let cfg = Config::resolve(paths(FilePaths::default()), None, |_| None).unwrap();
        assert_eq!(
            cfg.log_file.unwrap(),
            cfg.cache_dir.join("logs").join("victory.log")
        );
        let parsed: FileConfig = toml::from_str("[display]\nmode = 'sideways'\n").unwrap();
        let why = Config::resolve(parsed, None, |_| None).unwrap_err();
        assert!(why.to_string().contains("sideways"), "{why}");
        let env = |k: &str| (k == "VICTORY_VSYNC").then(|| OsString::from("maybe"));
        let why = Config::resolve(paths(FilePaths::default()), None, env).unwrap_err();
        assert!(why.to_string().contains("maybe"), "{why}");
    }

    #[test]
    fn cache_inside_repo_is_refused() {
        let inside = Path::new(env!("CARGO_MANIFEST_DIR")).join("cache");
        let cfg = Config {
            cache_dir: inside,
            ..Default::default()
        };
        assert!(cfg.cache_subdir("x").is_err());
    }

    #[test]
    fn data_inside_repo_is_refused() {
        let inside = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let cfg = Config {
            data_dir: inside,
            ..Default::default()
        };
        assert!(cfg.data_subdir("x").is_err());
    }
}
