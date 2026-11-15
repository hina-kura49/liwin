// SPDX-License-Identifier: MIT OR Apache-2.0
//! Configuration: `~/.config/liwin/liwin.toml`.
//!
//! Key names follow AeroSpace's `~/.aerospace.toml` so that a subset of an
//! AeroSpace config can be read later. Keys liwin does not know (including
//! macOS-only AeroSpace options) are reported as warnings, never errors.
//!
//! This is a skeleton: only the types, loading, and keybinding conversion
//! exist. Most values are not acted upon yet.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Root of `liwin.toml` (AeroSpace key names).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct Config {
    pub config_version: Option<u32>,
    /// AeroSpace: start at login. Accepted for compatibility; not used yet.
    pub start_at_login: Option<bool>,
    pub enable_normalization_flatten_containers: Option<bool>,
    pub enable_normalization_opposite_orientation_for_nested_containers: Option<bool>,
    pub default_root_container_layout: Option<Layout>,
    pub default_root_container_orientation: Option<OrientationSetting>,
    /// AeroSpace: commands run after startup. Not used yet.
    pub after_startup_command: Vec<String>,
    pub gaps: Gaps,
    /// Binding modes; `main` is the default mode.
    pub mode: BTreeMap<String, Mode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    Tiles,
    Accordion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrientationSetting {
    Horizontal,
    Vertical,
    Auto,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Gaps {
    pub inner: InnerGaps,
    pub outer: OuterGaps,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct InnerGaps {
    pub horizontal: Gap,
    pub vertical: Gap,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct OuterGaps {
    pub left: Gap,
    pub bottom: Gap,
    pub top: Gap,
    pub right: Gap,
}

/// A gap in pixels. AeroSpace also allows per-monitor arrays; they are
/// accepted but not interpreted yet.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Gap {
    Uniform(i32),
    PerMonitor(Vec<toml::Value>),
}

impl Default for Gap {
    fn default() -> Self {
        Gap::Uniform(0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Mode {
    /// `alt-h = 'focus left'` or `alt-h = ['focus left', 'mode main']`.
    pub binding: BTreeMap<String, BindingCommands>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum BindingCommands {
    One(String),
    Many(Vec<String>),
}

impl BindingCommands {
    pub fn commands(&self) -> Vec<&str> {
        match self {
            BindingCommands::One(c) => vec![c.as_str()],
            BindingCommands::Many(cs) => cs.iter().map(String::as_str).collect(),
        }
    }
}

/// A resolved keybinding of the `main` mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding {
    /// AeroSpace notation as written by the user, e.g. `alt-shift-h`.
    pub key: String,
    /// GTK accelerator for GNOME Shell, e.g. `<Alt><Shift>h`.
    pub accelerator: String,
    /// Commands, each split into arguments.
    pub commands: Vec<Vec<String>>,
}

/// Result of loading a config file.
#[derive(Debug, Default)]
pub struct Loaded {
    pub config: Config,
    /// Human-readable warnings (unknown keys, unsupported bindings, ...).
    pub warnings: Vec<String>,
}

/// `$XDG_CONFIG_HOME/liwin/liwin.toml`, falling back to `~/.config`.
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("liwin").join("liwin.toml"))
}

/// Parses config text. Unknown keys become warnings.
pub fn parse(text: &str) -> Result<Loaded, String> {
    let mut warnings = Vec::new();
    let de = toml::Deserializer::parse(text).map_err(|e| e.to_string())?;
    let config: Config = serde_ignored::deserialize(de, |path| {
        warnings.push(format!("unknown or unsupported key '{path}' ignored"));
    })
    .map_err(|e| e.to_string())?;
    Ok(Loaded { config, warnings })
}

/// Loads `path`. A missing file yields the default config.
pub fn load(path: &Path) -> Result<Loaded, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Loaded::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

impl Config {
    /// Resolves the `main` mode bindings. Invalid ones are reported in
    /// `warnings` and skipped.
    pub fn main_bindings(&self, warnings: &mut Vec<String>) -> Vec<KeyBinding> {
        let Some(main) = self.mode.get("main") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (key, cmds) in &main.binding {
            match to_accelerator(key) {
                Ok(accelerator) => out.push(KeyBinding {
                    key: key.clone(),
                    accelerator,
                    commands: cmds
                        .commands()
                        .into_iter()
                        .map(|c| c.split_whitespace().map(str::to_owned).collect())
                        .collect(),
                }),
                Err(e) => warnings.push(format!("binding '{key}' skipped: {e}")),
            }
        }
        out
    }
}

/// Converts AeroSpace key notation (`alt-shift-h`) to a GTK accelerator
/// (`<Alt><Shift>h`). AeroSpace's `cmd` maps to `<Super>`.
pub fn to_accelerator(notation: &str) -> Result<String, String> {
    let parts: Vec<&str> = notation.split('-').collect();
    let (key, modifiers) = parts
        .split_last()
        .filter(|(k, _)| !k.is_empty())
        .ok_or_else(|| "empty key".to_owned())?;
    let mut accel = String::new();
    for m in modifiers {
        accel.push_str(match *m {
            "alt" => "<Alt>",
            "ctrl" => "<Control>",
            "shift" => "<Shift>",
            "cmd" => "<Super>",
            other => return Err(format!("unknown modifier '{other}'")),
        });
    }
    accel.push_str(&key_name(key)?);
    Ok(accel)
}

fn key_name(key: &str) -> Result<String, String> {
    let named = match key {
        "minus" => "minus",
        "equal" => "equal",
        "period" => "period",
        "comma" => "comma",
        "slash" => "slash",
        "backslash" => "backslash",
        "quote" => "apostrophe",
        "semicolon" => "semicolon",
        "backtick" => "grave",
        "leftSquareBracket" => "bracketleft",
        "rightSquareBracket" => "bracketright",
        "space" => "space",
        "enter" => "Return",
        "esc" => "Escape",
        "backspace" => "BackSpace",
        "tab" => "Tab",
        "left" => "Left",
        "right" => "Right",
        "up" => "Up",
        "down" => "Down",
        _ => "",
    };
    if !named.is_empty() {
        return Ok(named.to_owned());
    }
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && (c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return Ok(c.to_string());
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=20).contains(&n)
    {
        return Ok(format!("F{n}"));
    }
    Err(format!("unsupported key '{key}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_default() {
        let loaded = parse("").unwrap();
        assert_eq!(loaded.config, Config::default());
        assert!(loaded.warnings.is_empty());
    }

    #[test]
    fn parses_aerospace_style_config() {
        let text = r#"
            config-version = 2
            start-at-login = true
            default-root-container-layout = 'tiles'
            default-root-container-orientation = 'auto'
            after-startup-command = []

            # macOS-only AeroSpace option: must only warn.
            automatically-unhide-macos-hidden-apps = false

            [gaps]
            inner.horizontal = 10
            inner.vertical = 10
            outer.left = 5
            outer.top = [{ monitor.main = 16 }, 24]

            [mode.main.binding]
            alt-h = 'focus left'
            alt-shift-l = ['focus right', 'focus up']
            cmd-enter = 'exec-and-forget open -n /System/Applications/Utilities/Terminal.app'
        "#;
        let loaded = parse(text).unwrap();
        let c = &loaded.config;
        assert_eq!(c.config_version, Some(2));
        assert_eq!(c.default_root_container_layout, Some(Layout::Tiles));
        assert_eq!(
            c.default_root_container_orientation,
            Some(OrientationSetting::Auto)
        );
        assert_eq!(c.gaps.inner.horizontal, Gap::Uniform(10));
        assert!(matches!(c.gaps.outer.top, Gap::PerMonitor(_)));
        assert_eq!(
            loaded.warnings,
            vec!["unknown or unsupported key 'automatically-unhide-macos-hidden-apps' ignored"]
        );

        let mut warnings = Vec::new();
        let bindings = c.main_bindings(&mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            bindings,
            vec![
                KeyBinding {
                    key: "alt-h".into(),
                    accelerator: "<Alt>h".into(),
                    commands: vec![vec!["focus".into(), "left".into()]],
                },
                KeyBinding {
                    key: "alt-shift-l".into(),
                    accelerator: "<Alt><Shift>l".into(),
                    commands: vec![
                        vec!["focus".into(), "right".into()],
                        vec!["focus".into(), "up".into()],
                    ],
                },
                KeyBinding {
                    key: "cmd-enter".into(),
                    accelerator: "<Super>Return".into(),
                    commands: vec![
                        "exec-and-forget open -n /System/Applications/Utilities/Terminal.app"
                            .split_whitespace()
                            .map(str::to_owned)
                            .collect()
                    ],
                },
            ]
        );
    }

    #[test]
    fn nested_unknown_keys_warn() {
        let loaded = parse("[gaps]\ninner.diagonal = 3\n[mode.main]\nfoo = 1\n").unwrap();
        assert_eq!(
            loaded.warnings,
            vec![
                "unknown or unsupported key 'gaps.inner.diagonal' ignored",
                "unknown or unsupported key 'mode.main.foo' ignored",
            ]
        );
    }

    #[test]
    fn wrong_types_are_errors() {
        assert!(parse("default-root-container-layout = 'spiral'").is_err());
        assert!(parse("config-version = 'two'").is_err());
    }

    #[test]
    fn accelerator_conversion() {
        assert_eq!(to_accelerator("alt-h").unwrap(), "<Alt>h");
        assert_eq!(to_accelerator("ctrl-alt-1").unwrap(), "<Control><Alt>1");
        assert_eq!(to_accelerator("alt-minus").unwrap(), "<Alt>minus");
        assert_eq!(to_accelerator("alt-f12").unwrap(), "<Alt>F12");
        assert_eq!(
            to_accelerator("alt-leftSquareBracket").unwrap(),
            "<Alt>bracketleft"
        );
        assert!(to_accelerator("hyper-h").is_err());
        assert!(to_accelerator("alt-").is_err());
        assert!(to_accelerator("alt-H").is_err());
    }

    #[test]
    fn invalid_bindings_are_skipped_with_warning() {
        let loaded =
            parse("[mode.main.binding]\nmeta-x = 'focus left'\nalt-j = 'focus down'\n").unwrap();
        let mut warnings = Vec::new();
        let bindings = loaded.config.main_bindings(&mut warnings);
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            warnings,
            vec!["binding 'meta-x' skipped: unknown modifier 'meta'"]
        );
    }

    #[test]
    fn missing_file_is_default() {
        let loaded = load(Path::new("/nonexistent/liwin/liwin.toml")).unwrap();
        assert_eq!(loaded.config, Config::default());
    }
}
