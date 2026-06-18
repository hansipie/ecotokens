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
        name: "claude-sonnet-4-6",
        input_usd_per_1m: 3.00,
        output_usd_per_1m: 15.00,
    },
    ModelDefinition {
        name: "claude-opus-4-8",
        input_usd_per_1m: 5.00,
        output_usd_per_1m: 25.00,
    },
    // --- OpenAI GPT ---
    ModelDefinition {
        name: "gpt-5.5",
        input_usd_per_1m: 5.00,
        output_usd_per_1m: 30.00,
    },
    ModelDefinition {
        name: "gpt-5.4",
        input_usd_per_1m: 2.50,
        output_usd_per_1m: 15.00,
    },
    ModelDefinition {
        name: "gpt-5.4-mini",
        input_usd_per_1m: 0.75,
        output_usd_per_1m: 4.50,
    },
    // --- Google Gemini ---
    ModelDefinition {
        name: "gemini-2.5-flash-lite",
        input_usd_per_1m: 0.10,
        output_usd_per_1m: 0.40,
    },
    ModelDefinition {
        name: "gemini-2.5-flash",
        input_usd_per_1m: 0.30,
        output_usd_per_1m: 2.50,
    },
    ModelDefinition {
        name: "gemini-3.1-flash-lite-preview",
        input_usd_per_1m: 0.25,
        output_usd_per_1m: 1.50,
    },
    ModelDefinition {
        name: "gemini-3-flash-preview",
        input_usd_per_1m: 0.50,
        output_usd_per_1m: 3.00,
    },
    // --- Mistral ---
    ModelDefinition {
        name: "mistral-medium-3.5",
        input_usd_per_1m: 1.50,
        output_usd_per_1m: 7.50,
    },
    ModelDefinition {
        name: "devstral-small",
        input_usd_per_1m: 0.10,
        output_usd_per_1m: 0.30,
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
