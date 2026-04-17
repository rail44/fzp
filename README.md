# fzp (Fuzzy Processor)

Parallel LLM pipe filter. Reads text lines from stdin, sends each to an LLM in parallel, writes results to stdout preserving input order.

```
cat items.txt | fzp "Classify into: bug, feature, question"
```

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
| `-j N` | Concurrency | 8 |
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

## Claude Code plugin

fzp is also available as a [Claude Code plugin](https://github.com/rail44/fzp). Add it to your project:

```bash
claude mcp add-plugin github.com/rail44/fzp
```

## License

MIT
