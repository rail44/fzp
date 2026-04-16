use clap::Parser;

#[derive(Parser)]
#[command(name = "hunch", about = "Parallel LLM inference pipe filter")]
pub struct Cli {
    /// Inline prompt (e.g. "Classify into: bug, feature, question")
    pub prompt: Option<String>,

    /// Use a named preset instead of inline prompt
    #[arg(long, short)]
    pub preset: Option<String>,

    /// Template variable for preset (e.g. -v labels="bug,feature")
    #[arg(long = "var", short = 'v', value_name = "KEY=VALUE")]
    pub vars: Vec<String>,

    /// Model name
    #[arg(long, short, default_value = "google/gemini-2.0-flash-lite-001")]
    pub model: String,

    /// Number of concurrent requests
    #[arg(long, short = 'j', default_value_t = 8)]
    pub concurrency: usize,
}
