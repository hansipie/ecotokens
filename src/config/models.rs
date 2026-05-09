use crate::config::settings::ModelPrice;
use std::collections::HashMap;

pub struct ModelDefinition {
    pub name: &'static str,
    pub input_usd_per_1m: f64,
    pub output_usd_per_1m: f64,
}

pub const MODELS: &[ModelDefinition] = &[
    // --- Anthropic Claude ---
    ModelDefinition {
        name: "claude-haiku-4-5",
        input_usd_per_1m: 1.00,
        output_usd_per_1m: 5.00,
    },
    ModelDefinition {
        name: "claude-haiku-4-5-20251001",
        input_usd_per_1m: 1.00,
        output_usd_per_1m: 5.00,
    },
    ModelDefinition {
        name: "claude-sonnet-4-5",
        input_usd_per_1m: 3.00,
        output_usd_per_1m: 15.00,
    },
    ModelDefinition {
        name: "claude-sonnet-4-6",
        input_usd_per_1m: 3.00,
        output_usd_per_1m: 15.00,
    },
    ModelDefinition {
        name: "claude-opus-4-6",
        input_usd_per_1m: 15.00,
        output_usd_per_1m: 75.00,
    },
    ModelDefinition {
        name: "claude-opus-4-7",
        input_usd_per_1m: 5.00,
        output_usd_per_1m: 25.00,
    },
    // --- OpenAI GPT ---
    ModelDefinition {
        name: "gpt-4o",
        input_usd_per_1m: 2.50,
        output_usd_per_1m: 10.00,
    },
    ModelDefinition {
        name: "gpt-4o-mini",
        input_usd_per_1m: 0.15,
        output_usd_per_1m: 0.60,
    },
    ModelDefinition {
        name: "gpt-4.1",
        input_usd_per_1m: 2.00,
        output_usd_per_1m: 8.00,
    },
    ModelDefinition {
        name: "gpt-4.1-mini",
        input_usd_per_1m: 0.40,
        output_usd_per_1m: 1.60,
    },
    ModelDefinition {
        name: "gpt-4.1-nano",
        input_usd_per_1m: 0.10,
        output_usd_per_1m: 0.40,
    },
    ModelDefinition {
        name: "gpt-5",
        input_usd_per_1m: 1.25,
        output_usd_per_1m: 10.00,
    },
    ModelDefinition {
        name: "gpt-5-mini",
        input_usd_per_1m: 0.25,
        output_usd_per_1m: 2.00,
    },
    ModelDefinition {
        name: "gpt-5-nano",
        input_usd_per_1m: 0.05,
        output_usd_per_1m: 0.40,
    },
    // --- OpenAI Reasoning ---
    ModelDefinition {
        name: "o1",
        input_usd_per_1m: 15.00,
        output_usd_per_1m: 60.00,
    },
    ModelDefinition {
        name: "o3",
        input_usd_per_1m: 2.00,
        output_usd_per_1m: 8.00,
    },
    ModelDefinition {
        name: "o4-mini",
        input_usd_per_1m: 1.10,
        output_usd_per_1m: 4.40,
    },
    // --- Google Gemini ---
    ModelDefinition {
        name: "gemini-2.5-pro",
        input_usd_per_1m: 1.25,
        output_usd_per_1m: 10.00,
    },
    ModelDefinition {
        name: "gemini-2.5-flash",
        input_usd_per_1m: 0.30,
        output_usd_per_1m: 2.50,
    },
    ModelDefinition {
        name: "gemini-2.5-flash-lite",
        input_usd_per_1m: 0.10,
        output_usd_per_1m: 0.40,
    },
    ModelDefinition {
        name: "gemini-2.0-flash",
        input_usd_per_1m: 0.10,
        output_usd_per_1m: 0.40,
    },
    // --- DeepSeek ---
    ModelDefinition {
        name: "deepseek-v3",
        input_usd_per_1m: 0.252,
        output_usd_per_1m: 0.378,
    },
    // --- Mistral ---
    ModelDefinition {
        name: "mistral-large",
        input_usd_per_1m: 0.50,
        output_usd_per_1m: 1.50,
    },
    ModelDefinition {
        name: "mistral-small",
        input_usd_per_1m: 0.15,
        output_usd_per_1m: 0.60,
    },
    // --- Meta Llama ---
    ModelDefinition {
        name: "llama-4-maverick",
        input_usd_per_1m: 0.15,
        output_usd_per_1m: 0.60,
    },
    ModelDefinition {
        name: "llama-4-scout",
        input_usd_per_1m: 0.08,
        output_usd_per_1m: 0.30,
    },
    ModelDefinition {
        name: "llama-3.3-70b-instruct",
        input_usd_per_1m: 0.10,
        output_usd_per_1m: 0.32,
    },
    // --- Alibaba Qwen ---
    ModelDefinition {
        name: "qwen3.6-max",
        input_usd_per_1m: 1.30,
        output_usd_per_1m: 7.80,
    },
    ModelDefinition {
        name: "qwen3.6-plus",
        input_usd_per_1m: 0.50,
        output_usd_per_1m: 3.00,
    },
    ModelDefinition {
        name: "qwen3.6-flash",
        input_usd_per_1m: 0.25,
        output_usd_per_1m: 1.50,
    },
    ModelDefinition {
        name: "qwen3.5-plus",
        input_usd_per_1m: 0.40,
        output_usd_per_1m: 2.40,
    },
    ModelDefinition {
        name: "qwen3.5-flash",
        input_usd_per_1m: 0.10,
        output_usd_per_1m: 0.40,
    },
    // Subscription-based: no per-token cost, token savings still tracked
    ModelDefinition {
        name: "github-copilot",
        input_usd_per_1m: 0.0,
        output_usd_per_1m: 0.0,
    },
];

pub fn build_pricing_map() -> HashMap<String, ModelPrice> {
    let mut m = HashMap::new();
    for model in MODELS {
        m.insert(
            model.name.to_string(),
            ModelPrice {
                input_usd_per_1m: model.input_usd_per_1m,
                output_usd_per_1m: model.output_usd_per_1m,
            },
        );
    }
    m
}

pub fn get_price(model: &str) -> Option<ModelPrice> {
    MODELS.iter().find(|m| m.name == model).map(|m| ModelPrice {
        input_usd_per_1m: m.input_usd_per_1m,
        output_usd_per_1m: m.output_usd_per_1m,
    })
}

pub fn model_names() -> Vec<&'static str> {
    MODELS.iter().map(|m| m.name).collect()
}
