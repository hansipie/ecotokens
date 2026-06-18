# Built-in model pricing

Prices used for cost calculations in `ecotokens gain`. All prices are per million tokens.

| Provider | Model | Input ($/1M) | Output ($/1M) |
|----------|-------|---:|---:|
| Anthropic | `claude-haiku-4-5` | 1.00 | 5.00 |
| Anthropic | `claude-sonnet-4-6` | 3.00 | 15.00 |
| Anthropic | `claude-opus-4-8` | 5.00 | 25.00 |
| OpenAI | `gpt-5.5` | 5.00 | 30.00 |
| OpenAI | `gpt-5.4` | 2.50 | 15.00 |
| OpenAI | `gpt-5.4-mini` | 0.75 | 4.50 |
| Google | `gemini-2.5-flash-lite` | 0.10 | 0.40 |
| Google | `gemini-2.5-flash` | 0.30 | 2.50 |
| Google | `gemini-3.1-flash-lite-preview` | 0.25 | 1.50 |
| Google | `gemini-3-flash-preview` | 0.50 | 3.00 |
| Mistral | `mistral-medium-3.5` | 1.50 | 7.50 |
| Mistral | `devstral-small` | 0.10 | 0.30 |

Override any entry or add a new model via `model_pricing` in `~/.config/ecotokens/config.json`:

```json
{
  "model_pricing": {
    "my-custom-model": { "input_usd_per_1m": 0.50, "output_usd_per_1m": 2.00 }
  }
}
```
