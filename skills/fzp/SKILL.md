---
name: fzp
description: |
  Parallel LLM pipe filter for processing 10+ independent text items using a
  lightweight model. Reads plain text lines from stdin, sends each to an LLM in
  parallel, writes results to stdout preserving input order. Use this to offload
  bulk classification, summarization, translation, filtering, and normalization
  from your context.
---

# fzp (Fuzzy Processor)

Process many text lines in parallel through a lightweight LLM. One input line produces one output line.

## When to use

- 10+ independent items to process
- Each item needs a lightweight judgment (< 5 seconds)
- Results are short (label, sentence, JSON)
- You don't need all results in your context (use jq/awk to aggregate)
- Read-only tasks only (no code changes)

## When NOT to use

- Fewer than 10 items (do it directly)
- Tasks requiring multi-turn reasoning
- Code modifications
- Tasks with dependencies between items

## Basic usage

```bash
# Inline prompt
<data> | fzp "Your prompt here"

# Named preset
<data> | fzp -p classify -v labels="bug,feature,question"
```

## Available presets

Run `fzp --list` to see all. Builtins:

- `classify` - Assign one label from a set. Vars: `labels`
- `summarize` - One-sentence summary
- `translate` - Translate text. Vars: `lang`
- `normalize` - Extract fields as compact JSON. Vars: `fields`
- `filter` - Output 1 (match) or 0 (no match). Vars: `condition`

## Options

- `-p NAME` preset name
- `-v KEY=VALUE` template variable (repeatable)
- `-m MODEL` model override
- `-j N` concurrency (default: 8)

## Pipeline patterns

```bash
# Classify and count
cat items.txt | fzp -p classify -v labels="bug,feature,question" | sort | uniq -c

# Filter with paste
paste items.txt <(cat items.txt | fzp -p filter -v condition="security-related") | awk -F'\t' '$2 == "1"' | cut -f1

# Summarize functions
rg -n '^func ' --type go | xargs -I{} sh -c 'echo "$1"' _ {} | fzp "One-line summary of this Go function"

# Normalize to JSON
cat messages.txt | fzp -p normalize -v fields="name,topic,urgency"
```

## Prerequisites

- `fzp` binary in PATH (`cargo install fzp`)
- `OPENROUTER_API_KEY` environment variable set
