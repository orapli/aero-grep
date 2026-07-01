use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
    HighContrast,
}

impl Theme {
    pub fn label(&self) -> &'static str {
        match self {
            Self::System => "System Default",
            Self::Dark => "Dark (Catppuccin Mocha)",
            Self::Light => "Light (Catppuccin Latte)",
            Self::HighContrast => "High Contrast",
        }
    }
}

/// Controls whether/how completed searches are recorded into History (#24).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Copy, Default)]
pub enum HistoryMode {
    /// Record every completed search automatically (current/legacy behavior).
    #[default]
    Auto,
    /// Record nothing automatically; an explicit "Save to history" action
    /// records the current search on demand.
    Manual,
    /// Never record; the History panel/toggle is hidden.
    Off,
}

impl HistoryMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Manual => "Manual",
            Self::Off => "Off",
        }
    }
}

fn default_font_size() -> f32 {
    13.0
}

fn default_backup() -> bool {
    true
}

fn default_confirm_before_replace() -> bool {
    true
}

fn default_exclude_dirs() -> String {
    ".git,target,node_modules,.svn,.hg,.idea,.vscode,build,dist".to_string()
}

fn default_wrap_lines() -> bool {
    false
}

fn default_reduce_motion() -> bool {
    false
}

fn default_respect_gitignore() -> bool {
    true
}

fn default_max_result_files() -> usize {
    2000
}

fn default_show_advanced() -> bool {
    false
}

fn default_backup_dir() -> String {
    dirs::config_dir()
        .map(|d| {
            d.join("aero-grep")
                .join("backups")
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default()
}

fn default_backup_retention_days() -> usize {
    7
}

fn default_search_encoding() -> String {
    "auto".to_string()
}

fn default_follow_symlinks() -> bool {
    false
}

fn default_search_hidden() -> bool {
    true
}

fn default_preset_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Preset {
    pub name: String,
    pub glob: String,
    #[serde(default = "default_preset_enabled")]
    pub enabled: bool,
}

pub fn default_presets() -> Vec<Preset> {
    vec![
        Preset {
            name: "Rust".to_string(),
            glob: "*.rs".to_string(),
            enabled: true,
        },
        Preset {
            name: "Python".to_string(),
            glob: "*.py".to_string(),
            enabled: true,
        },
        Preset {
            name: "JS/TS".to_string(),
            glob: "*.js,*.ts,*.jsx,*.tsx".to_string(),
            enabled: true,
        },
        Preset {
            name: "Web".to_string(),
            glob: "*.html,*.css,*.scss,*.js,*.ts,*.jsx,*.tsx,*.json,*.svg".to_string(),
            enabled: true,
        },
        Preset {
            name: "Config".to_string(),
            glob: "*.yaml,*.yml,*.json,*.toml,*.ini,*.xml,*.conf,*.config".to_string(),
            enabled: true,
        },
        Preset {
            name: "Go".to_string(),
            glob: "*.go".to_string(),
            enabled: true,
        },
        Preset {
            name: "Java".to_string(),
            glob: "*.java".to_string(),
            enabled: true,
        },
        Preset {
            name: "C/C++".to_string(),
            glob: "*.c,*.h,*.cpp,*.hpp,*.cc".to_string(),
            enabled: true,
        },
        Preset {
            name: "C#".to_string(),
            glob: "*.cs".to_string(),
            enabled: true,
        },
        Preset {
            name: "Ruby".to_string(),
            glob: "*.rb".to_string(),
            enabled: true,
        },
        Preset {
            name: "Kotlin".to_string(),
            glob: "*.kt,*.kts".to_string(),
            enabled: true,
        },
        Preset {
            name: "Swift".to_string(),
            glob: "*.swift".to_string(),
            enabled: true,
        },
        Preset {
            name: "Markdown".to_string(),
            glob: "*.md,*.markdown".to_string(),
            enabled: true,
        },
    ]
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum ExportOutputMode {
    #[default]
    Flat,
    Grouped,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum ExportPreset {
    #[default]
    Standard,
    IdeCompatible,
    ModernTree,
    Custom,
}

fn default_export_line_format() -> String {
    "%f:%l".to_string()
}

fn default_export_file_header_format() -> String {
    "%f".to_string()
}

fn default_export_header_format() -> String {
    "Query: %q%NDirectory: %d%NTime: %t%N---".to_string()
}

fn default_export_header_enabled() -> bool {
    false
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub history_limit: usize,
    pub max_file_size_mb: u64,
    #[serde(default)]
    pub editor_command: String,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_backup")]
    pub backup_before_replace: bool,
    #[serde(default = "default_confirm_before_replace")]
    pub confirm_before_replace: bool,
    #[serde(default = "default_exclude_dirs")]
    pub default_exclude_dirs: String,
    #[serde(default = "default_wrap_lines")]
    pub wrap_lines: bool,
    #[serde(default = "default_reduce_motion")]
    pub reduce_motion: bool,
    #[serde(default = "default_respect_gitignore")]
    pub respect_gitignore: bool,
    #[serde(default = "default_max_result_files")]
    pub max_result_files: usize,
    /// Show the advanced filter row (Include/Exclude/Regex/Case/Word/Context/Depth).
    #[serde(default = "default_show_advanced")]
    pub show_advanced: bool,
    #[serde(default = "default_backup_dir")]
    pub backup_dir: String,
    #[serde(default = "default_backup_retention_days")]
    pub backup_retention_days: usize,
    #[serde(default)]
    pub custom_font_path: String,
    #[serde(default = "default_presets")]
    pub presets: Vec<Preset>,
    #[serde(default)]
    pub export_preset: ExportPreset,
    #[serde(default)]
    pub export_output_mode: ExportOutputMode,
    #[serde(default = "default_export_line_format")]
    pub export_line_format: String,
    #[serde(default = "default_export_file_header_format")]
    pub export_file_header_format: String,
    #[serde(default = "default_export_header_format")]
    pub export_header_format: String,
    #[serde(default = "default_export_header_enabled")]
    pub export_header_enabled: bool,
    #[serde(default)]
    pub export_omit_single_file_name: bool,
    /// encoding_rs label (e.g. "shift_jis", "euc-jp") or "auto" for BOM sniffing only.
    #[serde(default = "default_search_encoding")]
    pub search_encoding: String,
    #[serde(default = "default_follow_symlinks")]
    pub follow_symlinks: bool,
    #[serde(default = "default_search_hidden")]
    pub search_hidden: bool,
    #[serde(default)]
    pub history_mode: HistoryMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            history_limit: 100,
            max_file_size_mb: 50,
            editor_command: String::new(),
            theme: Theme::default(),
            font_size: 13.0,
            backup_before_replace: true,
            confirm_before_replace: true,
            default_exclude_dirs: default_exclude_dirs(),
            wrap_lines: false,
            reduce_motion: false,
            respect_gitignore: true,
            max_result_files: 2000,
            show_advanced: false,
            backup_dir: default_backup_dir(),
            backup_retention_days: default_backup_retention_days(),
            custom_font_path: String::new(),
            presets: default_presets(),
            export_preset: ExportPreset::default(),
            export_output_mode: ExportOutputMode::default(),
            export_line_format: default_export_line_format(),
            export_file_header_format: default_export_file_header_format(),
            export_header_format: default_export_header_format(),
            export_header_enabled: default_export_header_enabled(),
            export_omit_single_file_name: false,
            search_encoding: default_search_encoding(),
            follow_symlinks: false,
            search_hidden: true,
            history_mode: HistoryMode::default(),
        }
    }
}

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("aero-grep").join("config.json"))
    }

    fn clamp_values(&mut self) {
        self.font_size = self.font_size.clamp(10.0, 24.0);
        self.history_limit = self.history_limit.clamp(1, 1000);
    }

    /// Resets to defaults while keeping user-created presets (#29).
    pub fn reset_preserving_presets(&mut self) {
        let presets = std::mem::take(&mut self.presets);
        *self = Self::default();
        self.presets = presets;
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        let mut cfg = if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        };
        cfg.clamp_values();
        cfg
    }

    pub fn save(&self) -> Result<()> {
        let Some(path) = Self::config_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serde_roundtrip() {
        let original = Config::default();
        let json = serde_json::to_string(&original).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(original.respect_gitignore, restored.respect_gitignore);
        assert_eq!(original.max_result_files, restored.max_result_files);
        assert_eq!(original.backup_dir, restored.backup_dir);
        assert_eq!(
            original.backup_retention_days,
            restored.backup_retention_days
        );
        assert_eq!(original.presets, restored.presets);
        assert_eq!(original.export_preset, restored.export_preset);
        assert_eq!(original.export_output_mode, restored.export_output_mode);
        assert_eq!(original.export_line_format, restored.export_line_format);
        assert_eq!(
            original.export_file_header_format,
            restored.export_file_header_format
        );
        assert_eq!(original.export_header_format, restored.export_header_format);
        assert_eq!(
            original.export_header_enabled,
            restored.export_header_enabled
        );
        assert_eq!(
            original.export_omit_single_file_name,
            restored.export_omit_single_file_name
        );
        assert_eq!(original.history_mode, restored.history_mode);
    }

    #[test]
    fn test_config_missing_new_fields_uses_defaults() {
        // Simulates an old config.json that predates respect_gitignore / max_result_files
        let old_json = r#"{"history_limit":100,"max_threads":4,"max_file_size_mb":50}"#;
        let cfg: Config = serde_json::from_str(old_json).unwrap();
        assert!(cfg.respect_gitignore);
        assert_eq!(cfg.max_result_files, 2000);
        assert!(cfg.backup_before_replace);
        assert!(cfg.confirm_before_replace);
        assert_eq!(cfg.backup_dir, default_backup_dir());
        assert_eq!(cfg.backup_retention_days, 7);
        assert_eq!(cfg.presets, default_presets());
        assert_eq!(cfg.export_preset, ExportPreset::Standard);
        assert_eq!(cfg.export_output_mode, ExportOutputMode::Flat);
        assert_eq!(cfg.export_line_format, "%f:%l");
        assert_eq!(cfg.export_file_header_format, "%f");
        assert_eq!(cfg.export_header_format, default_export_header_format());
        assert!(!cfg.export_header_enabled);
        assert!(!cfg.export_omit_single_file_name);
        assert_eq!(cfg.history_mode, HistoryMode::Auto);
    }

    // #24: history recording mode
    #[test]
    fn test_history_mode_default_is_auto() {
        assert_eq!(HistoryMode::default(), HistoryMode::Auto);
        assert_eq!(Config::default().history_mode, HistoryMode::Auto);
    }

    #[test]
    fn test_history_mode_serde_roundtrip() {
        for mode in [HistoryMode::Auto, HistoryMode::Manual, HistoryMode::Off] {
            let json = serde_json::to_string(&mode).unwrap();
            let restored: HistoryMode = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, mode);
        }
    }

    #[test]
    fn test_config_clamping() {
        let mut cfg = Config {
            font_size: 5.0,
            history_limit: 0,
            ..Config::default()
        };

        cfg.clamp_values();

        assert_eq!(cfg.font_size, 10.0);
        assert_eq!(cfg.history_limit, 1);

        // Another set of out of bounds values
        cfg.font_size = 50.0;
        cfg.history_limit = 2000;

        cfg.clamp_values();

        assert_eq!(cfg.font_size, 24.0);
        assert_eq!(cfg.history_limit, 1000);
    }

    #[test]
    fn test_reset_preserving_presets_keeps_presets_resets_rest() {
        let mut cfg = Config {
            font_size: 20.0,
            history_limit: 500,
            respect_gitignore: false,
            presets: vec![Preset {
                name: "Custom".to_string(),
                glob: "*.custom".to_string(),
                enabled: true,
            }],
            ..Config::default()
        };

        cfg.reset_preserving_presets();

        let defaults = Config::default();
        assert_eq!(cfg.font_size, defaults.font_size);
        assert_eq!(cfg.history_limit, defaults.history_limit);
        assert_eq!(cfg.respect_gitignore, defaults.respect_gitignore);
        assert_eq!(cfg.presets.len(), 1);
        assert_eq!(cfg.presets[0].name, "Custom");
    }
}
