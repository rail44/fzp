mod api;
mod cli;
mod pipeline;
mod preset;

use anyhow::{bail, Result};
use clap::Parser;
use cli::Cli;
use std::io::{self, BufReader};
use std::sync::Arc;

fn main() -> Result<()> {
    reset_sigpipe();

    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let cli = Cli::parse();

    let vars: Vec<(String, String)> = cli
        .vars
        .iter()
        .map(|s| {
            let (k, v) = s
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("invalid -v format: '{}', expected KEY=VALUE", s))?;
            Ok((k.to_string(), v.to_string()))
        })
        .collect::<Result<_>>()?;

    let config = preset::load_config()?;

    if cli.list {
        preset::list_prompts(&config);
        return Ok(());
    }

    let system_prompt =
        preset::resolve_prompt(cli.prompt.as_deref(), cli.preset.as_deref(), &vars, &config)?;

    let api_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
    if api_key.is_empty() {
        bail!("API key not found. Set the OPENROUTER_API_KEY environment variable.");
    }

    let base_url = std::env::var("FZP_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());

    let client = Arc::new(api::ApiClient::new(&base_url, api_key, cli.model));
    let input = Box::new(BufReader::new(io::stdin()));
    let output = Box::new(io::stdout());

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(pipeline::run(
        &system_prompt,
        client,
        cli.concurrency,
        input,
        output,
    ))?;

    Ok(())
}

#[cfg(unix)]
fn reset_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}
