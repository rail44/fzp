use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Deserialize)]
struct PresetFile {
    #[serde(flatten)]
    presets: HashMap<String, PresetDef>,
}

#[derive(Deserialize)]
struct PresetDef {
    prompt: String,
}

pub fn resolve_prompt(
    inline: Option<&str>,
    preset_name: Option<&str>,
    vars: &[(String, String)],
) -> Result<String> {
    match (inline, preset_name) {
        (Some(prompt), None) => Ok(prompt.to_string()),
        (None, Some(name)) => load_preset(name, vars),
        (Some(_), Some(_)) => bail!("cannot specify both inline prompt and --preset"),
        (None, None) => bail!("specify a prompt or use --preset"),
    }
}

fn load_preset(name: &str, vars: &[(String, String)]) -> Result<String> {
    let candidates = preset_paths();
    for path in &candidates {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let file: PresetFile = toml::from_str(&content)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            if let Some(def) = file.presets.get(name) {
                let mut prompt = def.prompt.clone();
                for (key, value) in vars {
                    prompt = prompt.replace(&format!("{{{{{key}}}}}"), value);
                }
                return Ok(prompt);
            }
        }
    }
    bail!(
        "preset '{}' not found. Searched: {}",
        name,
        candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn preset_paths() -> Vec<std::path::PathBuf> {
    let mut paths = vec![Path::new("hunch.toml").to_path_buf()];
    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("hunch").join("presets.toml"));
    }
    paths
}
