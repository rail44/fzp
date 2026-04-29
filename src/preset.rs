use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

static BUILTIN_TOML: &str = include_str!("builtin.toml");

#[derive(Deserialize, Default)]
pub struct Config {
    pub api_key: Option<String>,
    /// Shell command whose stdout is used as the API key (e.g. `pass show fzp/openrouter`).
    /// Used when `api_key` is not set, so the key never has to live in plaintext config.
    pub api_key_command: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub prompt: HashMap<String, PromptDef>,
}

#[derive(Deserialize, Clone)]
pub struct PromptDef {
    pub template: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Optional JSON Schema. When set, fzp sends `response_format` so the
    /// upstream constrains output to this shape (provider-dependent enforcement).
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct PromptResolution {
    pub system_prompt: String,
    pub output_schema: Option<serde_json::Value>,
}

pub fn load_config() -> Result<Config> {
    let builtin: Config =
        toml::from_str(BUILTIN_TOML).expect("builtin.toml is invalid");

    let path = config_path();
    if !path.exists() {
        return Ok(builtin);
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let user: Config =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))?;

    // User config overrides builtin
    let mut merged = builtin;
    merged.api_key = user.api_key;
    merged.api_key_command = user.api_key_command;
    merged.base_url = user.base_url;
    merged.model = user.model;
    for (name, def) in user.prompt {
        merged.prompt.insert(name, def);
    }
    Ok(merged)
}

pub fn resolve_api_key(config: &Config) -> Result<String> {
    if let Some(key) = config.api_key.as_deref().filter(|s| !s.is_empty()) {
        return Ok(key.to_string());
    }

    if let Some(cmd) = config.api_key_command.as_deref().filter(|s| !s.trim().is_empty()) {
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .with_context(|| format!("failed to execute api_key_command: {cmd}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("api_key_command failed ({}): {}", output.status, stderr.trim());
        }
        let key = String::from_utf8(output.stdout)
            .context("api_key_command produced non-UTF-8 output")?;
        let key = key.trim().to_string();
        if key.is_empty() {
            bail!("api_key_command produced empty output");
        }
        return Ok(key);
    }

    bail!("api_key not found. Set `api_key` or `api_key_command` in config (run `fzp init`).")
}

pub fn resolve_prompt(
    inline: Option<&str>,
    preset_name: Option<&str>,
    vars: &[(String, String)],
    config: &Config,
) -> Result<PromptResolution> {
    match (inline, preset_name) {
        (Some(prompt), None) => Ok(PromptResolution {
            system_prompt: prompt.to_string(),
            output_schema: None,
        }),
        (None, Some(name)) => load_preset(name, vars, config),
        (Some(extra), Some(name)) => {
            let mut resolved = load_preset(name, vars, config)?;
            resolved.system_prompt.push('\n');
            resolved.system_prompt.push_str(extra);
            Ok(resolved)
        }
        (None, None) => bail!("specify a prompt or use --preset"),
    }
}

pub fn list_prompts(config: &Config) {
    let builtin: Config =
        toml::from_str(BUILTIN_TOML).expect("builtin.toml is invalid");

    let mut names: Vec<&String> = config.prompt.keys().collect();
    names.sort();

    for name in names {
        let def = &config.prompt[name];
        let source = if builtin.prompt.contains_key(name) {
            if let Some(user_path) = user_config_path() {
                if user_path.exists() {
                    let content = std::fs::read_to_string(&user_path).unwrap_or_default();
                    let user: Config = toml::from_str(&content).unwrap_or_default();
                    if user.prompt.contains_key(name) {
                        "override"
                    } else {
                        "builtin"
                    }
                } else {
                    "builtin"
                }
            } else {
                "builtin"
            }
        } else {
            "custom"
        };

        let desc = def
            .description
            .as_deref()
            .unwrap_or(&def.template);
        println!("{name} [{source}] - {desc}");
    }
}

fn load_preset(name: &str, vars: &[(String, String)], config: &Config) -> Result<PromptResolution> {
    let def = config.prompt.get(name).ok_or_else(|| {
        anyhow::anyhow!(
            "prompt '{}' not found. Run `fzp --list` to see available prompts.",
            name,
        )
    })?;
    let mut prompt = def.template.clone();
    for (key, value) in vars {
        prompt = prompt.replace(&format!("{{{{{key}}}}}"), value);
    }
    Ok(PromptResolution {
        system_prompt: prompt,
        output_schema: def.output_schema.clone(),
    })
}

fn user_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config/fzp/config.toml"))
}

fn config_path() -> PathBuf {
    user_config_path().unwrap_or_else(|| PathBuf::from(".config/fzp/config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            api_key: None,
            api_key_command: None,
            base_url: None,
            model: None,
            prompt: HashMap::from([(
                "greet".to_string(),
                PromptDef {
                    template: "Say hello in {{lang}}".to_string(),
                    description: Some("Greeting".to_string()),
                    output_schema: None,
                },
            )]),
        }
    }

    #[test]
    fn resolve_inline_prompt() {
        let config = test_config();
        let result = resolve_prompt(Some("do something"), None, &[], &config).unwrap();
        assert_eq!(result.system_prompt, "do something");
        assert!(result.output_schema.is_none());
    }

    #[test]
    fn resolve_preset_with_vars() {
        let config = test_config();
        let vars = vec![("lang".to_string(), "Japanese".to_string())];
        let result = resolve_prompt(None, Some("greet"), &vars, &config).unwrap();
        assert_eq!(result.system_prompt, "Say hello in Japanese");
    }

    #[test]
    fn resolve_preset_not_found() {
        let config = test_config();
        let result = resolve_prompt(None, Some("nonexistent"), &[], &config);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_preset_with_extra_prompt() {
        let config = test_config();
        let vars = vec![("lang".to_string(), "Japanese".to_string())];
        let result = resolve_prompt(Some("Be concise"), Some("greet"), &vars, &config).unwrap();
        assert_eq!(result.system_prompt, "Say hello in Japanese\nBe concise");
    }

    #[test]
    fn resolve_neither_inline_nor_preset_errors() {
        let config = test_config();
        let result = resolve_prompt(None, None, &[], &config);
        assert!(result.is_err());
    }

    #[test]
    fn builtin_toml_parses() {
        let config: Config = toml::from_str(BUILTIN_TOML).unwrap();
        assert!(config.prompt.contains_key("classify"));
        assert!(config.prompt.contains_key("summarize"));
        assert!(config.prompt.contains_key("translate"));
        assert!(config.prompt.contains_key("normalize"));
        assert!(config.prompt.contains_key("filter"));
    }

    #[test]
    fn resolve_api_key_prefers_explicit_key() {
        let config = Config {
            api_key: Some("explicit-key".to_string()),
            api_key_command: Some("echo from-cmd".to_string()),
            ..Default::default()
        };
        let key = resolve_api_key(&config).unwrap();
        assert_eq!(key, "explicit-key");
    }

    #[test]
    fn resolve_api_key_runs_command_when_key_absent() {
        let config = Config {
            api_key_command: Some("printf 'cmd-key'".to_string()),
            ..Default::default()
        };
        let key = resolve_api_key(&config).unwrap();
        assert_eq!(key, "cmd-key");
    }

    #[test]
    fn resolve_api_key_treats_empty_key_as_unset() {
        let config = Config {
            api_key: Some(String::new()),
            api_key_command: Some("printf 'cmd-key'".to_string()),
            ..Default::default()
        };
        let key = resolve_api_key(&config).unwrap();
        assert_eq!(key, "cmd-key");
    }

    #[test]
    fn resolve_api_key_command_failure_propagates() {
        let config = Config {
            api_key_command: Some("false".to_string()),
            ..Default::default()
        };
        let err = resolve_api_key(&config).unwrap_err();
        assert!(err.to_string().contains("api_key_command failed"));
    }

    #[test]
    fn resolve_api_key_command_empty_output_errors() {
        let config = Config {
            api_key_command: Some("printf ''".to_string()),
            ..Default::default()
        };
        let err = resolve_api_key(&config).unwrap_err();
        assert!(err.to_string().contains("empty output"));
    }

    #[test]
    fn resolve_api_key_neither_set_errors() {
        let config = Config::default();
        let err = resolve_api_key(&config).unwrap_err();
        assert!(err.to_string().contains("api_key not found"));
    }

    #[test]
    fn template_multiple_vars() {
        let config = Config {
            prompt: HashMap::from([(
                "test".to_string(),
                PromptDef {
                    template: "{{a}} and {{b}}".to_string(),
                    description: None,
                    output_schema: None,
                },
            )]),
            ..Default::default()
        };
        let vars = vec![
            ("a".to_string(), "foo".to_string()),
            ("b".to_string(), "bar".to_string()),
        ];
        let result = resolve_prompt(None, Some("test"), &vars, &config).unwrap();
        assert_eq!(result.system_prompt, "foo and bar");
    }

    #[test]
    fn preset_output_schema_round_trips_from_toml() {
        let toml_src = r#"
[prompt.x]
template = "do thing"

[prompt.x.output_schema]
type = "object"
required = ["label"]

[prompt.x.output_schema.properties.label]
type = "string"
"#;
        let config: Config = toml::from_str(toml_src).unwrap();
        let resolved = resolve_prompt(None, Some("x"), &[], &config).unwrap();
        let schema = resolved.output_schema.expect("schema present");
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["required"][0], "label");
        assert_eq!(schema["properties"]["label"]["type"], "string");
    }
}
