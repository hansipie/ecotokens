# AI Summary Feature

## Overview

The `ai-summary` feature enables intelligent summarization of command outputs using a local Ollama LLM, with automatic fallback to generic head/tail filtering.

## Why AI Summary?

- **Semantic compression**: Keeps errors/warnings, removes verbose logs
- **Contextual**: Better than arbitrary head/tail for large outputs
- **Local & private**: Uses Ollama, no cloud API required

## Trade-offs

- ⚠️ **Latency**: +2-3s per command (Ollama API call)
- ⚠️ **Overhead**: Model inference cost
- ✅ **Benefit**: Only activates for outputs >2500 tokens where savings justify cost

## Installation

### 1. Install Ollama

```bash
# Linux/macOS
curl -fsSL https://ollama.ai/install.sh | sh

# Start Ollama service
ollama serve &
```

### 2. Pull a lightweight model

```bash
# Recommended: fast 3B parameter model
ollama pull llama3.2:3b

# Alternative: higher quality, slower
ollama pull llama3:8b
```

### 3. Build ecotokens with ai-summary

```bash
cargo build --release --features ai-summary
cargo install --path . --force --features ai-summary
```

## Configuration

Edit `~/.config/ecotokens/config.json`:

```json
{
  "ai_summary_enabled": true,
  "ai_summary_model": "llama3.2:3b",
  "embed_provider": {
    "ollama": {
      "url": "http://localhost:11434"
    }
  }
}
```

### Config Fields

- `ai_summary_enabled`: Enable/disable AI summarization (default: `false`)
- `ai_summary_model`: Ollama model to use (default: `"llama3.2:3b"`)
- `embed_provider`: Must be Ollama with URL pointing to running instance

## Behavior

### Activation criteria

AI summary is attempted **only** when:
1. Feature is compiled in (`--features ai-summary`)
2. `ai_summary_enabled: true` in config
3. Output size > 2500 tokens (~10KB)
4. Ollama is reachable at configured URL

### Fallback

If any condition fails (Ollama down, timeout, model error), **automatic fallback** to `force_filter_generic`:
- No user-visible errors
- Debug mode (`"debug": true`) logs reason to stderr

### Example

```bash
# Large pytest output (5000 tokens)
ecotokens filter -- pytest tests/

# Result:
# [ecotokens] AI summary (5143 → ~387 tokens):
# Test suite failed with 3 errors, 2 warnings:
# - test_auth.py::test_login: AssertionError on line 45
# - test_db.py::test_query: Timeout after 30s
# - test_api.py::test_create: 400 Bad Request
# Warnings: deprecated import in conftest.py, slow fixture
```

## Performance

| Model | Latency | Quality | Memory |
|-------|---------|---------|--------|
| llama3.2:3b | ~2-3s | Good | ~4GB |
| llama3:8b | ~5-8s | Better | ~8GB |

## Disabling

1. **Without rebuild**: Set `"ai_summary_enabled": false` in config
2. **Permanent**: Rebuild without feature: `cargo install --path . --force`

## Troubleshooting

### "AI summary skipped: Ollama not configured"
- Check `embed_provider` is set to Ollama in config
- Verify URL is correct

### "AI summary skipped: Output too small for AI summary"
- Normal: outputs <2500 tokens use generic filter (faster)

### "Ollama request failed: Connection refused"
- Ensure Ollama is running: `ollama serve`
- Check firewall/port 11434

### Slow performance
- Use a smaller model (llama3.2:3b)
- Reduce `num_predict` in `ai_summary.rs` (default: 500 tokens)
- Consider disabling for interactive workflows

## Testing

```bash
# Test with feature disabled
cargo test --test ai_summary_test

# Test with feature enabled
cargo test --test ai_summary_test --features ai-summary
```
