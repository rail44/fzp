---
name: fzp
description: |
  This skill should be used when the user needs to "classify items",
  "summarize many lines", "translate a list", "filter text by condition",
  "normalize data to JSON", or process 10+ independent text items in bulk
  through a lightweight LLM. Also trigger when the user mentions "fzp",
  "parallel LLM processing", "pipe filter", or wants to offload bulk
  text processing from the current context. Even if the user doesn't
  mention fzp by name, use this skill whenever batch text classification,
  summarization, translation, filtering, or normalization is needed.
---

# fzp (Fuzzy Processor)

Parallel LLM pipe filter. One input line produces one output line, processed in parallel through a lightweight model.

## When to use

- 10+ independent items to process
- Each item needs a lightweight judgment (< 5 seconds)
- Results are short (label, sentence, compact JSON)
- Aggregation via jq/awk/sort is sufficient (no need for all results in context)
- Read-only tasks only (no code changes)

## When NOT to use

- Fewer than 10 items (process directly in context)
- Tasks requiring multi-turn reasoning or cross-item dependencies
- Code modifications
- Tasks where every result must be in context for further reasoning

## Basic usage

```bash
# Inline prompt
<data> | fzp "Your prompt here"

# Named preset
<data> | fzp -p classify -v labels="bug,feature,question"

# Preset + extra instruction (combine preset with additional prompt)
<data> | fzp -p summarize "Respond in Japanese"
```

## Available presets

Run `fzp --list` to see all. Builtins:

| Preset | Description | Variables |
|--------|-------------|-----------|
| `classify` | Assign one label from a set | `labels` |
| `summarize` | One-sentence summary | - |
| `translate` | Translate text | `lang` |
| `normalize` | Extract fields as compact JSON | `fields` |
| `filter` | Output 1 (match) or 0 (no match) | `condition` |

Combine any preset with an extra inline prompt for additional instructions (e.g., language, format constraints).

## Options

| Flag | Description | Default |
|------|-------------|---------|
| `-p NAME` | Preset name | - |
| `-v KEY=VALUE` | Template variable (repeatable) | - |
| `-m MODEL` | Model override | from config.toml |
| `-j N` | Concurrency | 64 |
| `--list` | List available presets | - |

## Pipeline patterns

```bash
# Classify and count
cat items.txt | fzp -p classify -v labels="bug,feature,question" | sort | uniq -c

# Filter matching lines
paste items.txt <(cat items.txt | fzp -p filter -v condition="security-related") \
  | awk -F'\t' '$2 == "1"' | cut -f1

# Summarize with language override
cat messages.txt | fzp -p summarize "Respond in Japanese"

# Normalize to JSON
cat messages.txt | fzp -p normalize -v fields="name,topic,urgency"

# Translate
cat items.txt | fzp -p translate -v lang="French"
```

## Error handling

- Failed lines emit an empty line to preserve line alignment with input
- Errors are logged to stderr per line
- Process exits non-zero if any requests failed
- Summary printed to stderr: `fzp: N processed, M succeeded, K failed`

## Prerequisites

- `fzp` binary in PATH (`cargo install fzp`)
- `~/.config/fzp/config.toml` with `api_key` and `model` set (run `fzp init` to create)
