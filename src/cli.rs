use clap::Parser;

#[derive(Parser)]
#[command(name = "fzp", about = "Fuzzy Processor - parallel LLM inference pipe filter")]
pub struct Cli {
    /// Inline prompt (e.g. "Classify into: bug, feature, question")
    pub prompt: Option<String>,

    /// Use a named preset instead of inline prompt
    #[arg(long, short)]
    pub preset: Option<String>,

    /// Template variable for preset (e.g. -v labels="bug,feature")
    #[arg(long = "var", short = 'v', value_name = "KEY=VALUE")]
    pub vars: Vec<String>,

    /// Model name (overrides config.toml)
    #[arg(long, short)]
    pub model: Option<String>,

    /// Number of concurrent requests
    #[arg(long, short = 'j', default_value_t = 8)]
    pub concurrency: usize,

    /// List available prompts and exit
    #[arg(long)]
    pub list: bool,
}
