mod api;
mod cli;
mod pipeline;
mod preset;

use anyhow::{bail, Result};
use clap::Parser;
use cli::Cli;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
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

    let api_key = std::env::var(&cli.api_key_env).unwrap_or_default();
    if api_key.is_empty() {
        bail!(
            "API key not found. Set the {} environment variable.",
            cli.api_key_env
        );
    }

    let client = Arc::new(api::ApiClient::new(&cli.base_url, api_key, cli.model));

    let input: Box<dyn BufRead + Send> = match &cli.input {
        Some(path) => Box::new(BufReader::new(File::open(path)?)),
        None => Box::new(BufReader::new(io::stdin())),
    };

    let output: Box<dyn Write + Send> = match &cli.output {
        Some(path) => Box::new(BufWriter::new(File::create(path)?)),
        None => Box::new(BufWriter::new(io::stdout())),
    };

    let failures: Option<Box<dyn Write + Send>> = match &cli.failures {
        Some(path) => Some(Box::new(BufWriter::new(File::create(path)?))),
        None => None,
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(pipeline::run(
        &cli.task,
        client,
        cli.concurrency,
        input,
        output,
        failures,
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
