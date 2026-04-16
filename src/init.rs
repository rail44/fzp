use anyhow::{bail, Result};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

fn config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
    Ok(home.join(".config/fzp/config.toml"))
}

fn prompt_input(label: &str, default: Option<&str>) -> Result<String> {
    let suffix = match default {
        Some(d) => format!(" [{}]", d),
        None => String::new(),
    };
    eprint!("{}{}: ", label, suffix);
    io::stderr().flush()?;

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let line = line.trim().to_string();

    if line.is_empty() {
        match default {
            Some(d) => Ok(d.to_string()),
            None => bail!("{} is required", label),
        }
    } else {
        Ok(line)
    }
}

pub fn run() -> Result<()> {
    let path = config_path()?;

    if path.exists() {
        eprintln!("Config already exists: {}", path.display());
        eprint!("Overwrite? [y/N]: ");
        io::stderr().flush()?;
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        if !line.trim().eq_ignore_ascii_case("y") {
            return Ok(());
        }
    }

    let api_key = prompt_input("API key", None)?;
    let model = prompt_input("Model", Some("google/gemini-3.1-flash-lite-preview"))?;
    let base_url = prompt_input("Base URL", Some("https://openrouter.ai/api/v1"))?;

    let content = format!(
        "api_key = \"{}\"\nmodel = \"{}\"\nbase_url = \"{}\"\n",
        api_key.replace('\\', "\\\\").replace('"', "\\\""),
        model.replace('\\', "\\\\").replace('"', "\\\""),
        base_url.replace('\\', "\\\\").replace('"', "\\\""),
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content)?;
    eprintln!("Wrote {}", path.display());

    Ok(())
}
