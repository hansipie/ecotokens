# Built-in model pricing

Prices used for cost calculations in `ecotokens gain`. All prices are per million tokens.

| Provider | Model | Input ($/1M) | Output ($/1M) |
|----------|-------|---:|---:|
| Anthropic | `claude-haiku-4-5` / `claude-haiku-4-5-20251001` | 1.00 | 5.00 |
| Anthropic | `claude-sonnet-4-5` | 3.00 | 15.00 |
| Anthropic | `claude-sonnet-4-6` | 3.00 | 15.00 |
| Anthropic | `claude-opus-4-6` | 15.00 | 75.00 |
| Anthropic | `claude-opus-4-7` | 5.00 | 25.00 |
| OpenAI | `gpt-4o` | 2.50 | 10.00 |
| OpenAI | `gpt-4o-mini` | 0.15 | 0.60 |
| OpenAI | `gpt-4.1` | 2.00 | 8.00 |
| OpenAI | `gpt-4.1-mini` | 0.40 | 1.60 |
| OpenAI | `gpt-4.1-nano` | 0.10 | 0.40 |
| OpenAI | `gpt-5` | 1.25 | 10.00 |
| OpenAI | `gpt-5-mini` | 0.25 | 2.00 |
| OpenAI | `gpt-5-nano` | 0.05 | 0.40 |
| OpenAI | `o1` | 15.00 | 60.00 |
| OpenAI | `o3` | 2.00 | 8.00 |
| OpenAI | `o4-mini` | 1.10 | 4.40 |
| Google | `gemini-2.5-pro` | 1.25 | 10.00 |
| Google | `gemini-2.5-flash` | 0.30 | 2.50 |
| Google | `gemini-2.5-flash-lite` | 0.10 | 0.40 |
| Google | `gemini-2.0-flash` | 0.10 | 0.40 |
| DeepSeek | `deepseek-v3` | 0.252 | 0.378 |
| Mistral | `mistral-large` | 0.50 | 1.50 |
| Mistral | `mistral-small` | 0.15 | 0.60 |
| Meta | `llama-4-maverick` | 0.15 | 0.60 |
| Meta | `llama-4-scout` | 0.08 | 0.30 |
| Meta | `llama-3.3-70b-instruct` | 0.10 | 0.32 |
| Alibaba | `qwen3.6-max` | 1.30 | 7.80 |
| Alibaba | `qwen3.6-plus` | 0.50 | 3.00 |
| Alibaba | `qwen3.6-flash` | 0.25 | 1.50 |
| Alibaba | `qwen3.5-plus` | 0.40 | 2.40 |
| Alibaba | `qwen3.5-flash` | 0.10 | 0.40 |
| — | `github-copilot` | 0.00 | 0.00 |

Override any entry or add a new model via `model_pricing` in `~/.config/ecotokens/config.json`:

```json
{
  "model_pricing": {
    "my-custom-model": { "input_usd_per_1m": 0.50, "output_usd_per_1m": 2.00 }
  }
}
```
