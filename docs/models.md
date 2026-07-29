# Built-in model pricing

Prices used for cost calculations in `ecotokens gain`. All prices are per million tokens.

| Provider | Model | Input ($/1M) | Output ($/1M) |
|----------|-------|---:|---:|
| Anthropic | `claude-haiku-4-5` | 1.00 | 5.00 |
| Anthropic | `claude-sonnet-5` | 3.00 | 15.00 |
| Anthropic | `claude-opus-5` | 5.00 | 25.00 |
| Anthropic | `claude-fable-5` | 10.00 | 50.00 |
| OpenAI | `gpt-5.6-sol` | 5.00 | 30.00 |
| OpenAI | `gpt-5.6-terra` | 2.50 | 15.00 |
| OpenAI | `gpt-5.6-luna` | 1.00 | 6.00 |
| Google | `gemini-2.5-flash-lite` | 0.10 | 0.40 |
| Google | `gemini-2.5-flash` | 0.30 | 2.50 |
| Google | `gemini-3.5-flash-lite` | 0.30 | 2.50 |
| Google | `gemini-3.6-flash` | 1.50 | 7.50 |
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
