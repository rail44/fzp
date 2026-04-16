use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hunch", about = "Parallel LLM inference pipe filter")]
pub struct Cli {
    #[command(subcommand)]
    pub task: Task,

    /// Input file (default: stdin)
    #[arg(long, short)]
    pub input: Option<String>,

    /// Output file (default: stdout)
    #[arg(long, short)]
    pub output: Option<String>,

    /// File to write failed items to (omit to discard failures)
    #[arg(long)]
    pub failures: Option<String>,

    /// Number of concurrent requests
    #[arg(long, short = 'j', default_value_t = 8)]
    pub concurrency: usize,

    /// Model name
    #[arg(long, short, default_value = "google/gemini-2.0-flash-lite-001")]
    pub model: String,

    /// API base URL (OpenAI-compatible endpoint)
    #[arg(long, default_value = "https://openrouter.ai/api/v1")]
    pub base_url: String,

    /// Environment variable name for API key
    #[arg(long, default_value = "OPENROUTER_API_KEY")]
    pub api_key_env: String,
}

#[derive(Subcommand)]
pub enum Task {
    /// Classify items into given labels
    Classify {
        /// Comma-separated list of labels
        #[arg(long, short)]
        labels: String,
    },
    /// Extract structured JSON fields
    Extract {
        /// Comma-separated list of fields to extract
        #[arg(long, short)]
        fields: String,
    },
    /// Summarize each item in one sentence
    Summarize,
    /// Translate each item to the target language
    Translate {
        /// Target language
        #[arg(long, short)]
        lang: String,
    },
    /// Run a custom prompt
    Custom {
        /// Prompt template (use {{text}} as placeholder for input text)
        #[arg(long, short)]
        prompt: String,
    },
}
