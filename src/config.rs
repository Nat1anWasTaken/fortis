use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use serde_json::{Number as JsonNumber, Value as JsonValue};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::PathBuf;

/// A selectable option for `ConfigField::Select`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

impl SelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// Metadata for numeric configuration fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumberField {
    pub default: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub precision: Option<u8>,
    pub unit: Option<String>,
}

impl NumberField {
    pub fn new(default: f64) -> Self {
        Self {
            default,
            min: None,
            max: None,
            step: None,
            precision: None,
            unit: None,
        }
    }

    pub fn with_bounds(mut self, min: f64, max: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }

    pub fn with_step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }

    pub fn with_precision(mut self, precision: u8) -> Self {
        self.precision = Some(precision);
        self
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }
}

/// Metadata for free-form text configuration fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextField {
    pub default: String,
    pub placeholder: Option<String>,
    pub secret: bool,
    pub max_length: Option<usize>,
}

impl TextField {
    pub fn new(default: impl Into<String>) -> Self {
        Self {
            default: default.into(),
            placeholder: None,
            secret: false,
            max_length: None,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn secret(mut self, secret: bool) -> Self {
        self.secret = secret;
        self
    }

    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
        self
    }
}

/// Supported configuration field types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigField {
    Toggle {
        default: bool,
    },
    Number(NumberField),
    Select {
        default: String,
        options: Vec<SelectOption>,
    },
    Text(TextField),
}

/// A single configuration entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub field: ConfigField,
}

impl ConfigEntry {
    pub fn new(key: impl Into<String>, label: impl Into<String>, field: ConfigField) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            description: None,
            field,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Either a nested configuration group or a single entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigNode {
    Group(ConfigGroup),
    Entry(ConfigEntry),
}

/// A group of configuration entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigGroup {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub children: Vec<ConfigNode>,
}

impl ConfigGroup {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            description: None,
            children: Vec::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_children(mut self, children: Vec<ConfigNode>) -> Self {
        self.children = children;
        self
    }
}

/// Errors produced by the configuration manager.
#[derive(Debug)]
pub enum ConfigError {
    UnknownKey(String),
    TypeMismatch { key: String, expected: &'static str },
    ValidationFailed { key: String, message: String },
    Persistence(std::io::Error),
    Serialization(serde_json::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::UnknownKey(key) => write!(f, "unknown configuration key '{key}'"),
            ConfigError::TypeMismatch { key, expected } => {
                write!(f, "configuration '{key}' expects type {expected}")
            }
            ConfigError::ValidationFailed { key, message } => {
                write!(f, "configuration '{key}' failed validation: {message}")
            }
            ConfigError::Persistence(err) => write!(f, "failed to persist configuration: {err}"),
            ConfigError::Serialization(err) => {
                write!(f, "failed to serialize configuration: {err}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        ConfigError::Persistence(err)
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(err: serde_json::Error) -> Self {
        ConfigError::Serialization(err)
    }
}

/// Central configuration manager that stores state, metadata, and persistence.
#[derive(Debug)]
pub struct ConfigManager {
    root: ConfigGroup,
    lookup: HashMap<String, ConfigEntry>,
    values: HashMap<String, JsonValue>,
    storage_path: PathBuf,
}

const EPSILON: f64 = 1e-6;

impl ConfigManager {
    /// Build a configuration manager using the application's default schema.
    pub fn with_default_schema() -> Self {
        Self::new(default_schema())
    }

    /// Create a new configuration manager from the provided schema.
    pub fn new(root: ConfigGroup) -> Self {
        let mut manager = Self {
            root,
            lookup: HashMap::new(),
            values: HashMap::new(),
            storage_path: default_storage_path(),
        };

        manager.index_schema();
        manager.load_from_disk();

        manager
    }

    /// Access the root schema group.
    pub fn schema(&self) -> &ConfigGroup {
        &self.root
    }

    /// Retrieve metadata for a configuration entry.
    pub fn entry(&self, key: &str) -> Result<&ConfigEntry, ConfigError> {
        self.lookup
            .get(key)
            .ok_or_else(|| ConfigError::UnknownKey(key.to_string()))
    }

    /// Return the raw stored value (if any).
    fn stored_value(&self, key: &str) -> Option<&JsonValue> {
        self.values.get(key)
    }

    /// Read the boolean value for a toggle field.
    pub fn bool_value(&self, key: &str) -> Result<bool, ConfigError> {
        let entry = self.entry(key)?;
        match &entry.field {
            ConfigField::Toggle { default } => Ok(self
                .stored_value(key)
                .and_then(JsonValue::as_bool)
                .unwrap_or(*default)),
            _ => Err(ConfigError::TypeMismatch {
                key: key.to_string(),
                expected: "boolean",
            }),
        }
    }

    /// Update a boolean field and persist if it changed. Returns `true` when the value changed.
    pub fn set_bool(&mut self, key: &str, value: bool) -> Result<bool, ConfigError> {
        let entry = self.entry(key)?.clone();
        let ConfigField::Toggle { default } = entry.field else {
            return Err(ConfigError::TypeMismatch {
                key: key.to_string(),
                expected: "boolean",
            });
        };

        let changed = self
            .stored_value(key)
            .and_then(JsonValue::as_bool)
            .map(|current| current != value)
            .unwrap_or(value != default);

        if !changed {
            return Ok(false);
        }

        if value == default {
            self.values.remove(key);
        } else {
            self.values.insert(key.to_string(), JsonValue::Bool(value));
        }

        self.persist()?;
        Ok(true)
    }

    /// Convenience helper to toggle a boolean.
    pub fn toggle_bool(&mut self, key: &str) -> Result<bool, ConfigError> {
        let current = self.bool_value(key)?;
        self.set_bool(key, !current)
    }

    /// Read numeric value for number fields.
    pub fn number_value(&self, key: &str) -> Result<f64, ConfigError> {
        let entry = self.entry(key)?;
        let ConfigField::Number(number_field) = &entry.field else {
            return Err(ConfigError::TypeMismatch {
                key: key.to_string(),
                expected: "number",
            });
        };

        let value = self
            .stored_value(key)
            .and_then(JsonValue::as_f64)
            .unwrap_or(number_field.default);

        Ok(clamp_number(number_field, value))
    }

    /// Set numeric value and persist. Returns `true` when a change was written.
    pub fn set_number(&mut self, key: &str, value: f64) -> Result<bool, ConfigError> {
        let entry = self.entry(key)?.clone();
        let ConfigField::Number(field) = entry.field else {
            return Err(ConfigError::TypeMismatch {
                key: key.to_string(),
                expected: "number",
            });
        };

        let mut new_value = clamp_number(&field, value);
        new_value = round_to_precision(new_value, field.precision);

        let current = self.stored_value(key).and_then(JsonValue::as_f64);

        let default_equal = (new_value - field.default).abs() < EPSILON;
        let changed = match current {
            Some(current_value) => (current_value - new_value).abs() > EPSILON,
            None => !default_equal,
        };

        if !changed {
            return Ok(false);
        }

        if default_equal {
            self.values.remove(key);
        } else {
            let number =
                JsonNumber::from_f64(new_value).ok_or_else(|| ConfigError::ValidationFailed {
                    key: key.to_string(),
                    message: "number cannot be represented".to_string(),
                })?;
            self.values
                .insert(key.to_string(), JsonValue::Number(number));
        }

        self.persist()?;
        Ok(true)
    }

    /// Adjust numeric value by a multiple of its step. Returns `true` when changed.
    pub fn adjust_number(&mut self, key: &str, steps: f64) -> Result<bool, ConfigError> {
        let entry = self.entry(key)?;
        let ConfigField::Number(field) = &entry.field else {
            return Err(ConfigError::TypeMismatch {
                key: key.to_string(),
                expected: "number",
            });
        };

        let step_size = field.step.unwrap_or(1.0);
        let delta = step_size * steps;

        let current = self.number_value(key)?;
        let mut new_value = current + delta;
        new_value = clamp_number(field, new_value);
        new_value = round_to_precision(new_value, field.precision);

        if (new_value - current).abs() < EPSILON {
            return Ok(false);
        }

        self.set_number(key, new_value)
    }

    /// Read a select value as owned string.
    pub fn select_value(&self, key: &str) -> Result<String, ConfigError> {
        let entry = self.entry(key)?;
        let ConfigField::Select { default, options } = &entry.field else {
            return Err(ConfigError::TypeMismatch {
                key: key.to_string(),
                expected: "select",
            });
        };

        let value = self
            .stored_value(key)
            .and_then(JsonValue::as_str)
            .unwrap_or(default.as_str());

        if options.iter().any(|opt| opt.value == value) {
            Ok(value.to_string())
        } else {
            Ok(default.clone())
        }
    }

    /// Set a select value explicitly.
    pub fn set_select(&mut self, key: &str, value: &str) -> Result<bool, ConfigError> {
        let entry = self.entry(key)?.clone();
        let ConfigField::Select { default, options } = entry.field else {
            return Err(ConfigError::TypeMismatch {
                key: key.to_string(),
                expected: "select",
            });
        };

        if !options.iter().any(|opt| opt.value == value) {
            return Err(ConfigError::ValidationFailed {
                key: key.to_string(),
                message: format!("'{value}' is not a valid option"),
            });
        }

        let current = self
            .stored_value(key)
            .and_then(JsonValue::as_str)
            .unwrap_or(default.as_str());

        let changed = current != value;

        if !changed {
            return Ok(false);
        }

        if value == default {
            self.values.remove(key);
        } else {
            self.values
                .insert(key.to_string(), JsonValue::String(value.to_string()));
        }

        self.persist()?;
        Ok(true)
    }

    /// Cycle through select options in a direction (-1 or 1).
    pub fn cycle_select(&mut self, key: &str, direction: isize) -> Result<bool, ConfigError> {
        let entry = self.entry(key)?.clone();
        let ConfigField::Select { options, .. } = entry.field else {
            return Err(ConfigError::TypeMismatch {
                key: key.to_string(),
                expected: "select",
            });
        };

        if options.is_empty() {
            return Ok(false);
        }

        let current = self.select_value(key)?;
        let len = options.len() as isize;
        let current_index = options
            .iter()
            .position(|opt| opt.value == current)
            .unwrap_or(0) as isize;

        let mut next_index = current_index + direction;
        next_index = ((next_index % len) + len) % len;

        let next_value = options[next_index as usize].value.clone();
        self.set_select(key, &next_value)
    }

    /// Update the available options for a select field at runtime.
    pub fn update_select_options(
        &mut self,
        key: &str,
        options: Vec<SelectOption>,
        default: Option<String>,
    ) -> Result<(), ConfigError> {
        let default_ref = default.as_ref();
        if !update_select_options_in_group(&mut self.root, key, &options, default_ref)? {
            return Err(ConfigError::UnknownKey(key.to_string()));
        }

        self.index_schema();

        if options.is_empty() {
            self.values.remove(key);
            return Ok(());
        }

        let stored_valid = self
            .stored_value(key)
            .and_then(JsonValue::as_str)
            .map(|value| value.to_string())
            .filter(|value| options.iter().any(|opt| opt.value == *value));

        let effective_value = stored_valid
            .or(default)
            .or_else(|| options.first().map(|opt| opt.value.clone()))
            .unwrap_or_default();

        // Persist the effective value so the configuration remains valid.
        // Ignore the returned flag since we don't care whether it changed.
        let _ = self.set_select(key, &effective_value)?;
        Ok(())
    }

    /// Read text value for text fields.
    pub fn text_value(&self, key: &str) -> Result<String, ConfigError> {
        let entry = self.entry(key)?;
        let ConfigField::Text(field) = &entry.field else {
            return Err(ConfigError::TypeMismatch {
                key: key.to_string(),
                expected: "text",
            });
        };

        let value = self
            .stored_value(key)
            .and_then(JsonValue::as_str)
            .map(|s| s.to_string())
            .unwrap_or(field.default.clone());

        Ok(value)
    }

    /// Set text value and persist. Returns `true` when a change was written.
    pub fn set_text(&mut self, key: &str, value: &str) -> Result<bool, ConfigError> {
        let entry = self.entry(key)?.clone();
        let ConfigField::Text(field) = entry.field else {
            return Err(ConfigError::TypeMismatch {
                key: key.to_string(),
                expected: "text",
            });
        };

        if let Some(max) = field.max_length {
            if value.chars().count() > max {
                return Err(ConfigError::ValidationFailed {
                    key: key.to_string(),
                    message: format!("value exceeds maximum length of {max} characters"),
                });
            }
        }

        let current = self
            .stored_value(key)
            .and_then(JsonValue::as_str)
            .map(|s| s.to_string())
            .unwrap_or(field.default.clone());

        if current == value {
            return Ok(false);
        }

        if value == field.default {
            self.values.remove(key);
        } else {
            self.values
                .insert(key.to_string(), JsonValue::String(value.to_string()));
        }

        self.persist()?;
        Ok(true)
    }

    fn index_schema(&mut self) {
        self.lookup.clear();
        fn walk(lookup: &mut HashMap<String, ConfigEntry>, group: &ConfigGroup) {
            for node in &group.children {
                match node {
                    ConfigNode::Group(child_group) => walk(lookup, child_group),
                    ConfigNode::Entry(entry) => {
                        lookup.insert(entry.key.clone(), entry.clone());
                    }
                }
            }
        }
        walk(&mut self.lookup, &self.root);
    }

    fn load_from_disk(&mut self) {
        let path = self.storage_path.clone();
        if !path.exists() {
            return;
        }

        match fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<HashMap<String, JsonValue>>(&contents) {
                Ok(store) => {
                    for (key, value) in store {
                        if let Some(entry) = self.lookup.get(&key) {
                            if let Some(validated) = validate_value(entry, &value) {
                                self.values.insert(key, validated);
                            }
                        }
                    }
                }
                Err(err) => {
                    eprintln!(
                        "Warning: failed to parse configuration file '{}': {err}",
                        path.display()
                    );
                }
            },
            Err(err) => {
                eprintln!(
                    "Warning: failed to read configuration file '{}': {err}",
                    path.display()
                );
            }
        }
    }

    fn persist(&self) -> Result<(), ConfigError> {
        if let Some(parent) = self.storage_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let serialized = serde_json::to_string_pretty(&self.values)?;
        fs::write(&self.storage_path, serialized)?;
        Ok(())
    }
}

fn validate_value(entry: &ConfigEntry, value: &JsonValue) -> Option<JsonValue> {
    match &entry.field {
        ConfigField::Toggle { .. } => value.as_bool().map(JsonValue::Bool),
        ConfigField::Number(field) => {
            let num = value.as_f64()?;
            let clamped = clamp_number(field, num);
            let rounded = round_to_precision(clamped, field.precision);
            JsonNumber::from_f64(rounded).map(JsonValue::Number)
        }
        ConfigField::Select { options, .. } => value.as_str().and_then(|val| {
            if options.iter().any(|opt| opt.value == val) {
                Some(JsonValue::String(val.to_string()))
            } else {
                None
            }
        }),
        ConfigField::Text(field) => {
            let text = value.as_str()?;
            if let Some(max) = field.max_length {
                if text.chars().count() > max {
                    return None;
                }
            }
            Some(JsonValue::String(text.to_string()))
        }
    }
}

fn update_select_options_in_group(
    group: &mut ConfigGroup,
    key: &str,
    options: &[SelectOption],
    default: Option<&String>,
) -> Result<bool, ConfigError> {
    for child in &mut group.children {
        match child {
            ConfigNode::Group(child_group) => {
                if update_select_options_in_group(child_group, key, options, default)? {
                    return Ok(true);
                }
            }
            ConfigNode::Entry(entry) => {
                if entry.key == key {
                    match &mut entry.field {
                        ConfigField::Select {
                            default: entry_default,
                            options: entry_options,
                        } => {
                            *entry_options = options.to_vec();
                            if let Some(default_value) = default {
                                *entry_default = default_value.clone();
                            } else if !entry_options
                                .iter()
                                .any(|opt| opt.value == *entry_default)
                            {
                                if let Some(first) = entry_options.first() {
                                    *entry_default = first.value.clone();
                                }
                            }
                            return Ok(true);
                        }
                        _ => {
                            return Err(ConfigError::TypeMismatch {
                                key: key.to_string(),
                                expected: "select",
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(false)
}

fn clamp_number(field: &NumberField, value: f64) -> f64 {
    let mut result = value;
    if let Some(min) = field.min {
        result = result.max(min);
    }
    if let Some(max) = field.max {
        result = result.min(max);
    }
    result
}

fn round_to_precision(value: f64, precision: Option<u8>) -> f64 {
    if let Some(p) = precision {
        let factor = 10_f64.powi(p as i32);
        (value * factor).round() / factor
    } else {
        value
    }
}

fn default_storage_path() -> PathBuf {
    if let Some(dirs) = ProjectDirs::from("com", "Fortis", "Fortis") {
        dirs.config_dir().join("settings.json")
    } else {
        PathBuf::from("fortis_settings.json")
    }
}

fn audio_device_select_options() -> (String, Vec<SelectOption>) {
    match crate::audio::list_audio_devices() {
        Ok(devices) if !devices.is_empty() => {
            let default = devices
                .first()
                .cloned()
                .unwrap_or_else(|| "".to_string());
            let options = devices
                .into_iter()
                .map(|name| SelectOption::new(name.clone(), name))
                .collect();
            (default, options)
        }
        Ok(_) => {
            let placeholder_value = "__no_devices__".to_string();
            let options = vec![SelectOption::new(
                placeholder_value.clone(),
                "No input devices detected",
            )];
            (placeholder_value, options)
        }
        Err(err) => {
            eprintln!("Warning: failed to enumerate audio devices: {err}");
            let placeholder_value = "__no_devices__".to_string();
            let options = vec![SelectOption::new(
                placeholder_value.clone(),
                "Audio device enumeration failed",
            )];
            (placeholder_value, options)
        }
    }
}

fn default_schema() -> ConfigGroup {
    let (default_audio_device, audio_device_options) = audio_device_select_options();

    ConfigGroup::new("root", "Settings").with_children(vec![
        ConfigNode::Group(
            ConfigGroup::new("ui", "Interface")
                .with_description("Tune how the terminal interface behaves.")
                .with_children(vec![
                    ConfigNode::Group(ConfigGroup::new("ui.behavior", "Behavior").with_children(
                        vec![
                            ConfigNode::Entry(
                                ConfigEntry::new(
                                    "ui.behavior.auto_scroll",
                                    "Auto-scroll Transcripts",
                                    ConfigField::Toggle { default: true },
                                )
                                .with_description(
                                    "Keep the most recent transcription in view automatically.",
                                ),
                            ),
                            ConfigNode::Entry(
                                ConfigEntry::new(
                                    "ui.behavior.compact_mode",
                                    "Compact Layout",
                                    ConfigField::Toggle { default: false },
                                )
                                .with_description("Reduce spacing to fit more content on screen."),
                            ),
                        ],
                    )),
                    ConfigNode::Group(
                        ConfigGroup::new("ui.theme", "Theme")
                            .with_description("Personalize highlight and accent colors.")
                            .with_children(vec![
                                ConfigNode::Entry(
                                    ConfigEntry::new(
                                        "ui.theme.accent_color",
                                        "Accent Color",
                                        ConfigField::Select {
                                            default: "blue".into(),
                                            options: vec![
                                                SelectOption::new("blue", "Ocean Blue"),
                                                SelectOption::new("cyan", "Clear Cyan"),
                                                SelectOption::new("magenta", "Vibrant Magenta"),
                                                SelectOption::new("amber", "Warm Amber"),
                                                SelectOption::new("green", "Bright Green"),
                                            ],
                                        },
                                    )
                                    .with_description(
                                        "Highlight color used for selections and dialogs.",
                                    ),
                                ),
                                ConfigNode::Entry(
                                    ConfigEntry::new(
                                        "ui.theme.brightness",
                                        "Theme Brightness",
                                        ConfigField::Number(
                                            NumberField::new(1.0)
                                                .with_bounds(0.6, 1.4)
                                                .with_step(0.05)
                                                .with_precision(2),
                                        ),
                                    )
                                    .with_description(
                                        "Scale text brightness for better readability.",
                                    ),
                                ),
                            ]),
                    ),
                ]),
        ),
        ConfigNode::Group(
            ConfigGroup::new("audio", "Audio")
                .with_description("Control input capture characteristics.")
                .with_children(vec![ConfigNode::Group(
                    ConfigGroup::new("audio.input", "Input").with_children(vec![
                        ConfigNode::Entry(
                            ConfigEntry::new(
                                "audio.input.normalization_level",
                                "Normalization Level",
                                ConfigField::Number(
                                    NumberField::new(-18.0)
                                        .with_bounds(-40.0, 0.0)
                                        .with_step(1.0)
                                        .with_precision(0)
                                        .with_unit("dB"),
                                ),
                            )
                            .with_description(
                                "Target loudness (in dBFS) applied before streaming audio.",
                            ),
                        ),
                        ConfigNode::Entry(
                            ConfigEntry::new(
                                "audio.input.device",
                                "Input Device",
                                ConfigField::Select {
                                    default: default_audio_device,
                                    options: audio_device_options,
                                },
                            )
                            .with_description(
                                "Select the microphone or input device Fortis should use.",
                            ),
                        ),
                    ]),
                )]),
        ),
        ConfigNode::Group(
            ConfigGroup::new("transcriber", "Transcriber")
                .with_description("Configure speech-to-text providers.")
                .with_children(vec![ConfigNode::Group(
                    ConfigGroup::new("transcriber.deepgram", "Deepgram")
                        .with_description("Options for the Deepgram streaming API.")
                        .with_children(vec![
                            ConfigNode::Entry(
                                ConfigEntry::new(
                                    "transcriber.deepgram.api_key",
                                    "API Key",
                                    ConfigField::Text(
                                        TextField::new("")
                                            .with_placeholder("Falls back to DEEPGRAM_API_KEY")
                                            .secret(true)
                                            .with_max_length(128),
                                    ),
                                )
                                .with_description(
                                    "Override the DEEPGRAM_API_KEY environment variable with a stored key.",
                                ),
                            ),
                            ConfigNode::Entry(
                                ConfigEntry::new(
                                    "transcriber.deepgram.language",
                                    "Language",
                                    ConfigField::Select {
                                        default: "en-US".into(),
                                        options: vec![
                                            SelectOption::new("en-US", "English (US)"),
                                            SelectOption::new("en-GB", "English (UK)"),
                                            SelectOption::new("en", "English (Generic)"),
                                            SelectOption::new("es", "Spanish"),
                                            SelectOption::new("es-LATAM", "Spanish (LATAM)"),
                                            SelectOption::new("fr", "French"),
                                            SelectOption::new("de", "German"),
                                            SelectOption::new("it", "Italian"),
                                            SelectOption::new("pt-BR", "Portuguese (Brazil)"),
                                            SelectOption::new("hi", "Hindi"),
                                            SelectOption::new("ja", "Japanese"),
                                            SelectOption::new("ko", "Korean"),
                                        ],
                                    },
                                )
                                .with_description(
                                    "Primary language hint sent with Deepgram streaming requests.",
                                ),
                            ),
                        ]),
                )]),
        ),
    ])
}
