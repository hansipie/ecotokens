use rmcp::schemars;
use serde::Deserialize;

/// Deserialize an optional integer that may arrive as a JSON string or number.
/// Claude Code sometimes sends numeric parameters as strings (e.g. `"5"` instead of `5`).
fn de_opt_usize<'de, D>(d: D) -> Result<Option<usize>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(d)?;
    match v {
        None => Ok(None),
        Some(serde_json::Value::Number(n)) => n
            .as_u64()
            .map(|n| Some(n as usize))
            .ok_or_else(|| serde::de::Error::custom("expected non-negative integer")),
        Some(serde_json::Value::String(s)) => s
            .parse::<usize>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected number or string, got {other}"
        ))),
    }
}

fn de_opt_u32<'de, D>(d: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(d)?;
    match v {
        None => Ok(None),
        Some(serde_json::Value::Number(n)) => n
            .as_u64()
            .map(|n| Some(n as u32))
            .ok_or_else(|| serde::de::Error::custom("expected non-negative integer")),
        Some(serde_json::Value::String(s)) => {
            s.parse::<u32>().map(Some).map_err(serde::de::Error::custom)
        }
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected number or string, got {other}"
        ))),
    }
}

fn de_opt_f32<'de, D>(d: D) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(d)?;
    match v {
        None => Ok(None),
        Some(serde_json::Value::Number(n)) => n
            .as_f64()
            .map(|n| Some(n as f32))
            .ok_or_else(|| serde::de::Error::custom("expected number")),
        Some(serde_json::Value::String(s)) => {
            s.parse::<f32>().map(Some).map_err(serde::de::Error::custom)
        }
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected number or string, got {other}"
        ))),
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    #[schemars(description = "The search query")]
    pub query: String,
    #[schemars(description = "Maximum number of results (default: 5)")]
    #[serde(default, deserialize_with = "de_opt_usize")]
    pub top_k: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OutlineParams {
    #[schemars(description = "Path to a file or directory")]
    pub path: String,
    #[schemars(description = "Recursion depth for directories")]
    #[serde(default, deserialize_with = "de_opt_u32")]
    pub depth: Option<u32>,
    #[schemars(description = "Filter by symbol kinds (fn, struct, impl, etc.)")]
    pub kinds: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SymbolParams {
    #[schemars(description = "Stable symbol ID (e.g., src/lib.rs::greet#fn)")]
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraceParams {
    #[schemars(description = "Symbol name to trace")]
    pub symbol: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraceCalleesParams {
    #[schemars(description = "Symbol name to trace")]
    pub symbol: String,
    #[schemars(description = "Recursion depth (default: 1)")]
    #[serde(default, deserialize_with = "de_opt_u32")]
    pub depth: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DuplicatesParams {
    #[schemars(description = "Minimum similarity percentage to report (0–100, default: 70)")]
    #[serde(default, deserialize_with = "de_opt_f32")]
    pub threshold: Option<f32>,
    #[schemars(description = "Minimum code block size in lines (default: 5)")]
    #[serde(default, deserialize_with = "de_opt_usize")]
    pub min_lines: Option<usize>,
    #[schemars(description = "Maximum number of duplicate groups to return (default: 10)")]
    #[serde(default, deserialize_with = "de_opt_usize")]
    pub top_k: Option<usize>,
}
