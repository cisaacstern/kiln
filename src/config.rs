use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// Custom tsk binary path
    pub tsk_path: Option<String>,
    /// Custom tmux session name
    pub session_name: Option<String>,
    /// Default base branch
    pub default_base: Option<String>,
}

impl Config {
    /// Load config from ~/.config/kiln/config.toml, falling back to defaults
    pub fn load() -> Result<Self> {
        let config_path = config_file_path();
        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            let config: Config = toml_parse(&contents)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }
}

fn config_file_path() -> PathBuf {
    dirs_fallback().join("kiln").join("config.toml")
}

fn dirs_fallback() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        PathBuf::from("/tmp/.config")
    }
}

/// Minimal TOML parser for our simple config (key = "value" pairs only).
/// Avoids adding a toml crate dependency for a simple config file.
fn toml_parse(input: &str) -> Result<Config> {
    let mut config = Config::default();

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match key {
                "tsk_path" => config.tsk_path = Some(value.to_string()),
                "session_name" => config.session_name = Some(value.to_string()),
                "default_base" => config.default_base = Some(value.to_string()),
                _ => {} // ignore unknown keys
            }
        }
    }

    Ok(config)
}
