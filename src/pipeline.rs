use crate::api::ApiClient;
use anyhow::Result;
use futures_util::StreamExt;
use std::io::{BufRead, Write as IoWrite};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::warn;

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
    system_prompt: &str,
    client: Arc<ApiClient>,
    concurrency: usize,
    input: Box<dyn BufRead + Send>,
    mut output: Box<dyn IoWrite + Send>,
) -> Result<()> {
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

    let system_prompt = system_prompt.to_string();
    let mut stream = ReceiverStream::new(rx)
        .map(|(line_num, line)| {
            let client = client.clone();
            let system_prompt = system_prompt.clone();
            async move {
                if line.trim().is_empty() {
                    return (line_num, None);
                }

                match client.chat(&system_prompt, &line).await {
                    Ok(response) => (line_num, Some(Ok(response))),
                    Err(e) => (line_num, Some(Err(e.to_string()))),
                }
            }
        })
        .buffered(concurrency);

    while let Some((_line_num, result)) = stream.next().await {
        match result {
            Some(Ok(response)) => {
                progress.success.fetch_add(1, Ordering::Relaxed);
                if writeln!(output, "{response}").is_err() {
                    break; // SIGPIPE
                }
            }
            Some(Err(error)) => {
                progress.failed.fetch_add(1, Ordering::Relaxed);
                warn!("{error}");
            }
            None => {}
        }
        progress.print(false);
    }

    progress.print(true);
    Ok(())
}
