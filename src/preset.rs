use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

static BUILTIN_TOML: &str = include_str!("builtin.toml");

#[derive(Deserialize, Default)]
pub struct Config {
    pub api_key: Option<String>,
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
    merged.base_url = user.base_url;
    merged.model = user.model;
    for (name, def) in user.prompt {
        merged.prompt.insert(name, def);
    }
    Ok(merged)
}

pub fn resolve_prompt(
    inline: Option<&str>,
    preset_name: Option<&str>,
    vars: &[(String, String)],
    config: &Config,
) -> Result<String> {
    match (inline, preset_name) {
        (Some(prompt), None) => Ok(prompt.to_string()),
        (None, Some(name)) => load_preset(name, vars, config),
        (Some(_), Some(_)) => bail!("cannot specify both inline prompt and --preset"),
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

fn load_preset(name: &str, vars: &[(String, String)], config: &Config) -> Result<String> {
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
    Ok(prompt)
}

fn user_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config/fzp/config.toml"))
}

fn config_path() -> PathBuf {
    user_config_path().unwrap_or_else(|| PathBuf::from(".config/fzp/config.toml"))
}
