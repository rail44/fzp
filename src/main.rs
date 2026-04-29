mod api;
mod cli;
mod init;
mod pipeline;
mod preset;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use rustc_hash::FxHashMap;
use std::io::{self, BufReader};
use std::sync::{Arc, Mutex};

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

    if let Some(Command::Init) = cli.command {
        return init::run();
    }

    let run = &cli.run;

    let vars: Vec<(String, String)> = run
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

    if run.list {
        preset::list_prompts(&config);
        return Ok(());
    }

    let resolved =
        preset::resolve_prompt(run.prompt.as_deref(), run.preset.as_deref(), &vars, &config)?;

    let api_key = preset::resolve_api_key(&config)?;

    let base_url = config
        .base_url
        .as_deref()
        .unwrap_or("https://openrouter.ai/api/v1");

    let model = run.model.as_deref().or(config.model.as_deref()).ok_or_else(|| {
        anyhow::anyhow!("model not specified. Run `fzp init` or use -m")
    })?;

    let client = Arc::new(api::ApiClient::new(
        base_url,
        api_key,
        model.to_string(),
        resolved.output_schema,
    ));
    let cache = run
        .cache
        .then(|| Arc::new(Mutex::new(FxHashMap::default())));
    let input = Box::new(BufReader::new(io::stdin()));
    let output = Box::new(io::stdout());

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(pipeline::run(
        &resolved.system_prompt,
        client,
        run.concurrency,
        cache,
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
