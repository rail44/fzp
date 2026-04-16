use crate::api::ChatClient;
use anyhow::{bail, Result};
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
    fn print_summary(&self) {
        let success = self.success.load(Ordering::Relaxed);
        let failed = self.failed.load(Ordering::Relaxed);
        let total = success + failed;
        eprintln!("fzp: {total} processed, {success} succeeded, {failed} failed");
    }
}

pub async fn run<C: ChatClient>(
    system_prompt: &str,
    client: Arc<C>,
    concurrency: usize,
    input: Box<dyn BufRead + Send>,
    mut output: Box<dyn IoWrite + Send>,
) -> Result<()> {
    let progress = Arc::new(Progress {
        success: AtomicUsize::new(0),
        failed: AtomicUsize::new(0),
    });

    let (tx, rx) = mpsc::channel::<String>(concurrency * 2);
    tokio::task::spawn_blocking(move || {
        for (idx, line) in input.lines().enumerate() {
            match line {
                Ok(l) => {
                    if tx.blocking_send(l).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!(line = idx + 1, "read error: {e}");
                    break;
                }
            }
        }
    });

    let system_prompt = system_prompt.to_string();
    let mut stream = ReceiverStream::new(rx)
        .map(|line| {
            let client = client.clone();
            let system_prompt = system_prompt.clone();
            async move {
                if line.trim().is_empty() {
                    return None;
                }

                match client.chat(&system_prompt, &line).await {
                    Ok(response) => Some(Ok(response)),
                    Err(e) => Some(Err(e.to_string())),
                }
            }
        })
        .buffered(concurrency);

    while let Some(result) = stream.next().await {
        match result {
            Some(Ok(response)) => {
                progress.success.fetch_add(1, Ordering::Relaxed);
                let response = response.replace('\n', " ").replace('\r', "");
                if writeln!(output, "{response}").is_err() {
                    break; // SIGPIPE
                }
            }
            Some(Err(error)) => {
                progress.failed.fetch_add(1, Ordering::Relaxed);
                warn!("{error}");
                if writeln!(output).is_err() {
                    break;
                }
            }
            None => {
                if writeln!(output).is_err() {
                    break;
                }
            }
        }
    }

    let failed = progress.failed.load(Ordering::Relaxed);
    progress.print_summary();
    if failed > 0 {
        bail!("{failed} request(s) failed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    struct MockClient {
        handler: Box<dyn Fn(&str) -> Result<String> + Send + Sync>,
    }

    impl ChatClient for MockClient {
        async fn chat(&self, _system_prompt: &str, user_message: &str) -> Result<String> {
            (self.handler)(user_message)
        }
    }

    fn ok_client() -> Arc<MockClient> {
        Arc::new(MockClient {
            handler: Box::new(|msg| Ok(format!("echo:{msg}"))),
        })
    }

    fn failing_client() -> Arc<MockClient> {
        Arc::new(MockClient {
            handler: Box::new(|_| bail!("api error")),
        })
    }

    fn run_pipeline(client: Arc<MockClient>, input: &'static str) -> (Result<()>, String) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let output = Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
        let output_clone = output.clone();

        struct SharedWriter(Arc<std::sync::Mutex<Vec<u8>>>);
        impl IoWrite for SharedWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let result = rt.block_on(run(
            "test prompt",
            client,
            2,
            Box::new(BufReader::new(input.as_bytes())),
            Box::new(SharedWriter(output_clone)),
        ));
        let bytes = output.lock().unwrap().clone();
        (result, String::from_utf8(bytes).unwrap())
    }

    #[test]
    fn successful_lines() {
        let (result, output) = run_pipeline(ok_client(), "hello\nworld\n");
        assert!(result.is_ok());
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines, vec!["echo:hello", "echo:world"]);
    }

    #[test]
    fn empty_lines_preserved() {
        let (result, output) = run_pipeline(ok_client(), "hello\n\nworld\n");
        assert!(result.is_ok());
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "echo:hello");
        assert_eq!(lines[1], "");
        assert_eq!(lines[2], "echo:world");
    }

    #[test]
    fn failed_lines_emit_empty() {
        let (result, output) = run_pipeline(failing_client(), "hello\nworld\n");
        assert!(result.is_err());
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.is_empty()));
    }

    #[test]
    fn output_line_count_matches_input() {
        let client = Arc::new(MockClient {
            handler: Box::new(|msg| {
                if msg == "fail" {
                    bail!("error")
                } else {
                    Ok(format!("ok:{msg}"))
                }
            }),
        });
        let (_, output) = run_pipeline(client, "a\nfail\nb\n\nc\n");
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn newlines_in_response_normalized() {
        let client = Arc::new(MockClient {
            handler: Box::new(|_| Ok("line1\nline2\r\nline3".to_string())),
        });
        let (result, output) = run_pipeline(client, "test\n");
        assert!(result.is_ok());
        assert_eq!(output.trim(), "line1 line2 line3");
    }
}
