use crate::api::ChatClient;
use anyhow::{bail, Result};
use futures_util::StreamExt;
use rustc_hash::FxHashMap;
use std::io::{BufRead, Write as IoWrite};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::warn;

pub type LineCache = Arc<Mutex<FxHashMap<String, String>>>;

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
    cache: Option<LineCache>,
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

    let system_prompt: Arc<str> = system_prompt.into();
    let mut stream = ReceiverStream::new(rx)
        .map(|line| {
            let client = client.clone();
            let system_prompt = system_prompt.clone();
            let cache = cache.clone();
            async move {
                if line.trim().is_empty() {
                    return None;
                }

                if let Some(c) = &cache
                    && let Some(hit) = c.lock().unwrap().get(&line).cloned()
                {
                    return Some(Ok(hit));
                }

                match client.chat(&system_prompt, &line).await {
                    Ok(response) => {
                        if let Some(c) = &cache {
                            // first-writer-wins: if another task inserted while we were
                            // awaiting, drop our result and adopt the existing one.
                            let entry = c
                                .lock()
                                .unwrap()
                                .entry(line)
                                .or_insert_with(|| response.clone())
                                .clone();
                            return Some(Ok(entry));
                        }
                        Some(Ok(response))
                    }
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
        run_pipeline_with(client, input, None)
    }

    fn run_pipeline_with(
        client: Arc<MockClient>,
        input: &'static str,
        cache: Option<LineCache>,
    ) -> (Result<()>, String) {
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
            cache,
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

    fn counting_client() -> (Arc<MockClient>, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();
        let client = Arc::new(MockClient {
            handler: Box::new(move |msg| {
                count_clone.fetch_add(1, Ordering::Relaxed);
                Ok(format!("echo:{msg}"))
            }),
        });
        (client, count)
    }

    #[test]
    fn cache_dedups_repeated_lines() {
        let (client, count) = counting_client();
        let cache: LineCache = Arc::new(Mutex::new(FxHashMap::default()));
        let (result, output) = run_pipeline_with(client, "a\na\na\n", Some(cache));
        assert!(result.is_ok());
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines, vec!["echo:a", "echo:a", "echo:a"]);
        // Allow up to 2 calls because concurrent tasks may race past the lookup
        // before the first writer inserts. With concurrency=2 and 3 lines this
        // is the realistic upper bound.
        let calls = count.load(Ordering::Relaxed);
        assert!(calls <= 2, "expected at most 2 API calls, got {calls}");
    }

    #[test]
    fn cache_disabled_calls_per_line() {
        let (client, count) = counting_client();
        let (result, _) = run_pipeline_with(client, "a\na\na\n", None);
        assert!(result.is_ok());
        assert_eq!(count.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn cache_does_not_collapse_distinct_lines() {
        let (client, count) = counting_client();
        let cache: LineCache = Arc::new(Mutex::new(FxHashMap::default()));
        let (result, output) = run_pipeline_with(client, "a\nb\nc\n", Some(cache));
        assert!(result.is_ok());
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines, vec!["echo:a", "echo:b", "echo:c"]);
        assert_eq!(count.load(Ordering::Relaxed), 3);
    }
}
