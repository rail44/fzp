# hunch

大量かつ軽量な推論タスク(分類、抽出、要約、翻訳など)を、低コスト・高速な LLM を使って並列実行する Unix パイプフィルタ。Claude Code のセッションから呼び出して、親 Claude の文脈とコストから読み取り系の map 操作を剥がすために使う。

## 位置づけ

**名前の由来**: `hunch` は「直感、軽い判断」を意味する英単語。軽量モデルが出力する「大量の小さな直感」を、パイプで流し込んで並列に得る道具、というコンセプトから命名した。

**役割分担**:

- **親 Claude Code** — オーケストレーターと意思決定者。コード変更はこちらが担う
- **`hunch`** — 読み取り系の map 操作を軽量 LLM に剥がす道具
- **Unix の他のツール(jq、rg、awk など)** — 前処理・後処理を担う

## 設計哲学

- **Unix パイプフィルタ** — stdin で受け取り、stdout に返す。jq や awk と同じ流儀
- **読み取り専用** — コード変更や破壊的操作はしない
- **map 専門** — reduce(集約・判断)は親 Claude が担う
- **薄く広く** — 複雑な機能を持たず、他のツールとの組み合わせで力を発揮する

詳細は `docs/DESIGN.md` を、判断の背景は `docs/context/design-discussion.md` を参照。

## 想定する使い方(概念)

```
# パイプラインの一部として(基本形)
<データ源> | hunch <タスク指定> | <後処理>
```

具体例(**インターフェースの詳細は実装時に決定**):

```bash
# 関数群に一行要約を付ける
rg --json 'fn |function |def ' src/ \
  | jq -c '...前処理...' \
  | hunch ...(タスク指定) \
  | jq -c '...後処理...'

# Issue を分類する
cat issues.jsonl | hunch ...(分類タスク指定) > classified.jsonl
```

**重要**: 上記の CLI 形式は例示であり、正確なフラグ名・プリセット名・入出力形式は `docs/INTERFACE.md` の方針に基づいて Claude Code による実装時に決定する。

## ディレクトリ構成

```
.
├── README.md                     ← このファイル
├── docs/
│   ├── DESIGN.md                 ← 全体設計と方針
│   ├── REQUIREMENTS.md           ← MVP 要件と将来機能
│   ├── INTERFACE.md              ← CLI インターフェース設計方針
│   ├── EXAMPLES.md               ← 代表的な使用パターン
│   └── context/
│       └── design-discussion.md  ← 設計議論の記録(これが最も重要)
├── .claude/
│   ├── skills/
│   │   └── hunch/
│   │       ├── SKILL.md          ← Claude Code への指示
│   │       └── README.md         ← Skill の人間向け説明
│   └── agents/
│       └── design-advisor.md     ← 設計相談用サブエージェント
├── src/                          ← Rust 実装(Claude Code が作成)
├── examples/
│   └── sample-input.jsonl        ← 動作確認用サンプル
└── Cargo.toml                    ← Claude Code が作成
```

## 次にやること

1. Claude Code セッションをこのリポジトリのルートで起動する
2. 最初のプロンプトとして `docs/` 配下、特に `docs/context/design-discussion.md` を読ませる
3. 要約を確認してから、`docs/REQUIREMENTS.md` の MVP から 1 項目ずつ実装していく

初期プロンプトの例は `docs/context/design-discussion.md` の末尾に記載。

## ライセンス

未設定(プロジェクト方針に応じて決定)。
