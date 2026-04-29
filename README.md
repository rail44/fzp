# fzp (Fuzzy Processor)

Parallel LLM pipe filter. Reads text lines from stdin, sends each to an LLM in parallel, writes results to stdout preserving input order.

```
cat items.txt | fzp "Classify into: bug, feature, question"
```

## Scope

fzp targets **1-shot batch tasks**: classify, extract, translate, normalize. It
deliberately stays small — multi-turn conversations, agent loops, and tool
calling are out of scope. For those, reach for a Claude / OpenAI / Gemini SDK
directly.

## Install

```bash
cargo install fzp
```

## Setup

fzp uses any OpenAI-compatible API. It's designed for lightweight, fast models.

```bash
fzp init
```

This creates `~/.config/fzp/config.toml` with your API key, model, and endpoint.

To avoid storing the key in plaintext, replace `api_key` with `api_key_command`,
whose stdout is used as the key:

```toml
api_key_command = "pass show fzp/openrouter"
```

`api_key` takes precedence when both are set.

## Usage

```bash
# Inline prompt
<data> | fzp "Your prompt here"

# Named preset
<data> | fzp -p classify -v labels="bug,feature,question"

# Preset + extra instruction
<data> | fzp -p summarize "Respond in Japanese"
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-p NAME` | Preset name | - |
| `-v KEY=VALUE` | Template variable (repeatable) | - |
| `-m MODEL` | Model override | from config.toml |
| `-j N` | Concurrency | 64 |
| `--cache` | Dedup identical input lines (skip duplicate API calls) | off |
| `--list` | List available presets | - |

### Built-in presets

| Preset | Description | Variables |
|--------|-------------|-----------|
| `classify` | Assign one label from a set | `labels` |
| `summarize` | One-sentence summary | - |
| `translate` | Translate text | `lang` |
| `normalize` | Extract fields as compact JSON | `fields` |
| `filter` | Output 1 (match) or 0 (no match) | `condition` |

### Examples

```bash
# Classify and count
cat items.txt | fzp -p classify -v labels="bug,feature,question" | sort | uniq -c

# Filter
paste items.txt <(cat items.txt | fzp -p filter -v condition="security-related") \
  | awk -F'\t' '$2 == "1"' | cut -f1

# Normalize to JSON
cat messages.txt | fzp -p normalize -v fields="name,topic,urgency"
```

## Custom presets

Add presets to `~/.config/fzp/config.toml`:

```toml
[prompt.my-preset]
template = "Your prompt with {{var}}"
```

### Structured output

Attach a JSON Schema to constrain the model's output (provider-dependent
enforcement; OpenRouter routes to providers that honor it):

```toml
[prompt.extract]
template = "Extract name and age."

[prompt.extract.output_schema]
type = "object"
required = ["name", "age"]
additionalProperties = false

[prompt.extract.output_schema.properties.name]
type = "string"

[prompt.extract.output_schema.properties.age]
type = "integer"
```

## Claude Code plugin

fzp is also available as a [Claude Code plugin](https://github.com/rail44/fzp). Add it to your project:

```bash
claude mcp add-plugin github.com/rail44/fzp
```

## License

MIT
