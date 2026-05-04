use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;

fn best_device() -> Device {
    #[cfg(feature = "cuda")]
    if let Ok(d) = Device::new_cuda(0) {
        return d;
    }
    #[cfg(feature = "metal")]
    if let Ok(d) = Device::new_metal(0) {
        return d;
    }
    Device::Cpu
}

/// Local inference embedding provider using `all-MiniLM-L6-v2` (384 dim).
///
/// Model weights (~90 MB) are downloaded from HuggingFace on first use and
/// cached in `~/.cache/ecotokens/models/`. Subsequent calls are instant.
pub struct CandleProvider {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl CandleProvider {
    /// Initialise the provider for a given HuggingFace model ID.
    /// Downloads the model if not already cached.
    pub fn new(model_id: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let device = best_device();

        let (tokenizer_path, config_path, model_path) = acquire_model_files(model_id)?;

        let mut tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("tokenizer load error: {e}"))?;

        use tokenizers::TruncationParams;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: 512,
                ..Default::default()
            }))
            .map_err(|e| format!("truncation setup error: {e}"))?;

        let config_str = std::fs::read_to_string(&config_path)?;
        let config: Config = serde_json::from_str(&config_str)?;

        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&[model_path], DType::F32, &device)? };
        let model = BertModel::load(vb, &config)?;

        Ok(CandleProvider {
            model,
            tokenizer,
            device,
        })
    }

    /// Embed a single text. Convenience wrapper around `embed_batch`.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
        let mut batch = self.embed_batch(&[text])?;
        batch.pop().ok_or_else(|| "empty embed_batch result".into())
    }

    /// Embed a batch of texts. Returns one L2-normalised 384-dim vector per text.
    pub fn embed_batch(
        &self,
        texts: &[&str],
    ) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error + Send + Sync>> {
        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            let encoding = self
                .tokenizer
                .encode(*text, true)
                .map_err(|e| format!("encode error: {e}"))?;

            let ids: Vec<u32> = encoding.get_ids().to_vec();
            let mask: Vec<u32> = encoding.get_attention_mask().to_vec();
            let type_ids: Vec<u32> = encoding.get_type_ids().to_vec();

            let seq_len = ids.len();
            if seq_len == 0 {
                results.push(vec![0.0f32; 384]);
                continue;
            }

            let ids_t = Tensor::from_vec(ids, (1, seq_len), &self.device)?;
            let type_t = Tensor::from_vec(type_ids, (1, seq_len), &self.device)?;
            let mask_t = Tensor::from_vec(mask, (1, seq_len), &self.device)?;

            // Forward pass → [1, seq_len, hidden_dim]
            let output = self.model.forward(&ids_t, &type_t, Some(&mask_t))?;

            // Mean pooling weighted by attention mask
            let mask_f = mask_t.to_dtype(DType::F32)?.unsqueeze(2)?;
            let masked = output.broadcast_mul(&mask_f)?;
            let sum = masked.sum(1)?;
            let token_count = mask_t.to_dtype(DType::F32)?.sum_keepdim(1)?;
            let mean = sum.broadcast_div(&token_count)?;

            // L2 normalisation
            let norm = mean.sqr()?.sum_keepdim(1)?.sqrt()?;
            let normalised = mean.broadcast_div(&norm)?;

            let vec: Vec<f32> = normalised.squeeze(0)?.to_vec1()?;
            results.push(vec);
        }

        Ok(results)
    }
}

// ── Model file acquisition ────────────────────────────────────────────────────

/// Resolve local paths for the three model files, downloading if needed.
///
/// Lookup order:
///   1. HuggingFace hub cache (`~/.cache/huggingface/hub/…`) — populated by
///      other tools or a previous ecotokens run using the hf_hub path.
///   2. ecotokens own cache (`~/.cache/ecotokens/models/…`) — written by the
///      download path below.
///   3. Fresh download via reqwest (handles relative Location redirects that
///      hf_hub 0.3.x fails on).
pub(crate) fn acquire_model_files(
    model_id: &str,
) -> Result<(PathBuf, PathBuf, PathBuf), Box<dyn std::error::Error + Send + Sync>> {
    // 1. hf_hub cache (cache-only lookup, no network)
    if let Some(paths) = try_hf_cache(model_id) {
        return Ok(paths);
    }

    // 2. ecotokens cache
    let cache_dir = ecotokens_model_dir(model_id)?;
    let tok = cache_dir.join("tokenizer.json");
    let cfg = cache_dir.join("config.json");
    let mdl = cache_dir.join("model.safetensors");
    if tok.exists() && cfg.exists() && mdl.exists() {
        return Ok((tok, cfg, mdl));
    }

    // 3. Download
    let base = format!("https://huggingface.co/{model_id}/resolve/main");
    for (filename, dest) in [
        ("tokenizer.json", &tok),
        ("config.json", &cfg),
        ("model.safetensors", &mdl),
    ] {
        if !dest.exists() {
            download_file(&format!("{base}/{filename}"), dest, filename)?;
        }
    }

    Ok((tok, cfg, mdl))
}

/// Check the standard HuggingFace hub cache without any network request.
fn try_hf_cache(model_id: &str) -> Option<(PathBuf, PathBuf, PathBuf)> {
    use hf_hub::{Cache, Repo, RepoType};
    let cache_repo = Cache::default().repo(Repo::new(model_id.to_string(), RepoType::Model));
    let tok = cache_repo.get("tokenizer.json")?;
    let cfg = cache_repo.get("config.json")?;
    let mdl = cache_repo.get("model.safetensors")?;
    Some((tok, cfg, mdl))
}

/// Return (and create) ecotokens' own model cache directory for `model_id`.
fn ecotokens_model_dir(
    model_id: &str,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let dir = dirs::cache_dir()
        .ok_or("cannot determine cache directory")?
        .join("ecotokens")
        .join("models")
        .join(model_id.replace('/', "--"));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Download a single file via reqwest (follows redirects, including relative ones).
fn download_file(
    url: &str,
    dest: &Path,
    display_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use indicatif::{ProgressBar, ProgressStyle};

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;

    let mut response = client.get(url).send()?;
    if !response.status().is_success() {
        return Err(format!("HTTP {} fetching {url}", response.status()).into());
    }

    let size = response
        .content_length()
        .ok_or("could not determine content length")?;

    eprintln!("ecotokens: downloading {display_name}…");

    let pb = ProgressBar::new(size);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.blue} [{bar:40.green/white}] {bytes}/{total_bytes} ({percent}%) · {bytes_per_sec} · eta {eta}",
        )
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✓"])
        .progress_chars("██░"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    let tmp = dest.with_extension("tmp");
    let mut file = std::fs::File::create(&tmp)?;
    let mut writer = pb.wrap_write(&mut file);
    std::io::copy(&mut response, &mut writer)?;
    pb.finish_and_clear();

    std::fs::rename(&tmp, dest)?;
    Ok(())
}
