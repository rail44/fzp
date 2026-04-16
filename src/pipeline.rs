use crate::api::ApiClient;
use crate::cli::Task;
use crate::preset;
use anyhow::Result;
use futures_util::StreamExt;
use serde::Serialize;
use std::io::{BufRead, Write as IoWrite};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::warn;

#[derive(Serialize)]
struct FailureItem {
    line: usize,
    text: String,
    error: String,
}

struct Progress {
    success: AtomicUsize,
    failed: AtomicUsize,
}

impl Progress {
    fn print(&self, finished: bool) {
        let success = self.success.load(Ordering::Relaxed);
        let failed = self.failed.load(Ordering::Relaxed);
        let total = success + failed;
        if finished {
            eprintln!("\rhunch: {total} processed, {success} succeeded, {failed} failed");
        } else {
            eprint!("\rhunch: {total} processed, {success} succeeded, {failed} failed");
        }
    }
}

pub async fn run(
    task: &Task,
    client: Arc<ApiClient>,
    concurrency: usize,
    input: Box<dyn BufRead + Send>,
    mut output: Box<dyn IoWrite + Send>,
    mut failures: Option<Box<dyn IoWrite + Send>>,
) -> Result<()> {
    let system_prompt = preset::build_system_prompt(task);
    let progress = Arc::new(Progress {
        success: AtomicUsize::new(0),
        failed: AtomicUsize::new(0),
    });

    let (tx, rx) = mpsc::channel::<(usize, String)>(concurrency * 2);
    tokio::task::spawn_blocking(move || {
        for (line_num, line) in input.lines().enumerate() {
            match line {
                Ok(l) => {
                    if tx.blocking_send((line_num, l)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!(line = line_num + 1, "read error: {e}");
                    break;
                }
            }
        }
    });

    let mut stream = ReceiverStream::new(rx)
        .map(|(line_num, line)| {
            let client = client.clone();
            let system_prompt = system_prompt.clone();
            async move {
                if line.trim().is_empty() {
                    return (line_num, line, None);
                }

                match client.chat(&system_prompt, &line).await {
                    Ok(response) => (line_num, line, Some(Ok(response))),
                    Err(e) => (line_num, line, Some(Err(e.to_string()))),
                }
            }
        })
        .buffered(concurrency);

    while let Some((line_num, text, result)) = stream.next().await {
        match result {
            Some(Ok(response)) => {
                progress.success.fetch_add(1, Ordering::Relaxed);
                if writeln!(output, "{response}").is_err() {
                    break; // SIGPIPE
                }
            }
            Some(Err(error)) => {
                progress.failed.fetch_add(1, Ordering::Relaxed);
                if let Some(ref mut f) = failures {
                    let failure = FailureItem {
                        line: line_num + 1,
                        text,
                        error,
                    };
                    let json = serde_json::to_string(&failure).unwrap_or_default();
                    let _ = writeln!(f, "{json}");
                }
            }
            None => {} // empty line
        }
        progress.print(false);
    }

    progress.print(true);
    Ok(())
}
