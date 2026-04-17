use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

const MODEL_FILENAME: &str = "ibm-granite_granite-3.2-2b-instruct-Q4_K_M.gguf";
const MODEL_URL: &str = "https://huggingface.co/bartowski/ibm-granite_granite-3.2-2b-instruct-GGUF/resolve/main/ibm-granite_granite-3.2-2b-instruct-Q4_K_M.gguf";

#[derive(Debug, Clone, serde::Serialize)]
pub enum AiStatus {
    NotSetUp,
    Downloading { progress_pct: f32 },
    Loading,
    Ready,
    Error(String),
}

pub struct GenerateParams {
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub stop_sequences: Vec<String>,
}

struct LoadedModel {
    backend: Arc<LlamaBackend>,
    model: Arc<LlamaModel>,
}

pub struct LlmEngine {
    state: Arc<Mutex<EngineState>>,
    models_dir: PathBuf,
}

enum EngineState {
    Idle,
    Downloading(f32),
    Loading,
    Ready(LoadedModel),
    Failed(String),
}

impl LlmEngine {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let models_dir = app_data_dir.join("models");
        std::fs::create_dir_all(&models_dir).ok();
        Self {
            state: Arc::new(Mutex::new(EngineState::Idle)),
            models_dir,
        }
    }

    fn model_path(&self) -> PathBuf {
        self.models_dir.join(MODEL_FILENAME)
    }

    pub async fn get_status(&self) -> AiStatus {
        let state = self.state.lock().await;
        match &*state {
            EngineState::Idle => {
                if self.model_path().exists() {
                    AiStatus::NotSetUp
                } else {
                    AiStatus::NotSetUp
                }
            }
            EngineState::Downloading(pct) => AiStatus::Downloading { progress_pct: *pct },
            EngineState::Loading => AiStatus::Loading,
            EngineState::Ready(_) => AiStatus::Ready,
            EngineState::Failed(e) => AiStatus::Error(e.clone()),
        }
    }

    fn tmp_path(&self) -> PathBuf {
        self.models_dir.join(format!("{}.tmp", MODEL_FILENAME))
    }

    fn is_model_valid(&self) -> bool {
        let path = self.model_path();
        if !path.exists() { return false; }
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        // GGUF files have a magic header "GGUF" (4 bytes) and must be >1MB to be valid
        if size < 1_000_000 { return false; }
        if let Ok(mut f) = std::fs::File::open(&path) {
            let mut magic = [0u8; 4];
            if std::io::Read::read_exact(&mut f, &mut magic).is_ok() {
                return &magic == b"GGUF";
            }
        }
        false
    }

    pub async fn ensure_ready(&self) -> Result<(), String> {
        {
            let state = self.state.lock().await;
            if matches!(&*state, EngineState::Ready(_)) {
                return Ok(());
            }
        }

        // Clean up any partial downloads from previous crashes
        let tmp = self.tmp_path();
        if tmp.exists() {
            let _ = tokio::fs::remove_file(&tmp).await;
        }

        // If model file exists but is corrupted/partial, delete and re-download
        let path = self.model_path();
        if path.exists() && !self.is_model_valid() {
            let _ = tokio::fs::remove_file(&path).await;
        }

        if !self.model_path().exists() {
            self.download_model().await?;
        }

        if !self.is_model_valid() {
            let _ = tokio::fs::remove_file(&self.model_path()).await;
            return Err("Downloaded model file is corrupted. Please try again.".into());
        }

        self.load_model().await
    }

    async fn download_model(&self) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            *state = EngineState::Downloading(0.0);
        }

        let path = self.model_path();
        let tmp_path = self.tmp_path();
        let state = self.state.clone();

        let result = tokio::task::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3600))
                .connect_timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| format!("HTTP client error: {e}"))?;

            let res = client.get(MODEL_URL)
                .header("User-Agent", "Rustymail (desktop email client)")
                .send()
                .await
                .map_err(|e| format!("Download request failed: {e}"))?;

            if !res.status().is_success() {
                return Err(format!("Model download failed (HTTP {}). Please check your internet connection and try again.", res.status()));
            }

            let total = res.content_length().unwrap_or(0);
            if total > 0 && total < 1_000_000 {
                return Err("Server returned an unexpectedly small file. The model URL may be incorrect.".into());
            }

            let mut downloaded: u64 = 0;
            let mut file = tokio::fs::File::create(&tmp_path).await
                .map_err(|e| format!("Failed to create download file: {e}"))?;

            use tokio::io::AsyncWriteExt;
            use futures_util::StreamExt;
            let mut stream = res.bytes_stream();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| format!("Download interrupted: {e}. Please try again."))?;
                file.write_all(&chunk).await
                    .map_err(|e| format!("Failed to save download: {e}"))?;
                downloaded += chunk.len() as u64;
                if total > 0 {
                    let pct = (downloaded as f32 / total as f32) * 100.0;
                    let mut s = state.lock().await;
                    *s = EngineState::Downloading(pct);
                }
            }

            file.flush().await.map_err(|e| format!("Failed to finalize download: {e}"))?;
            drop(file);

            // Verify downloaded size matches expected
            let actual_size = tokio::fs::metadata(&tmp_path).await
                .map(|m| m.len())
                .unwrap_or(0);
            if total > 0 && actual_size != total {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(format!(
                    "Download incomplete ({:.1}MB of {:.1}MB). Please try again.",
                    actual_size as f64 / 1_048_576.0,
                    total as f64 / 1_048_576.0
                ));
            }

            tokio::fs::rename(&tmp_path, &path).await
                .map_err(|e| format!("Failed to save model file: {e}"))?;

            Ok::<(), String>(())
        })
        .await
        .map_err(|e| format!("Download task error: {e}"))?;

        if let Err(e) = result {
            let mut state = self.state.lock().await;
            *state = EngineState::Failed(e.clone());
            let _ = tokio::fs::remove_file(&self.tmp_path()).await;
            return Err(e);
        }

        Ok(())
    }

    async fn load_model(&self) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            *state = EngineState::Loading;
        }

        let path = self.model_path();
        let result = tokio::task::spawn_blocking(move || -> Result<LoadedModel, String> {
            let mut backend = LlamaBackend::init()
                .map_err(|e| format!("Backend init failed: {e}"))?;
            backend.void_logs();
            let model_params = LlamaModelParams::default().with_n_gpu_layers(99);
            let model = LlamaModel::load_from_file(&backend, &path, &model_params)
                .map_err(|e| format!("Model load failed: {e}"))?;
            Ok(LoadedModel {
                backend: Arc::new(backend),
                model: Arc::new(model),
            })
        })
        .await
        .map_err(|e| format!("Spawn error: {e}"))?;

        match result {
            Ok(loaded) => {
                let mut state = self.state.lock().await;
                *state = EngineState::Ready(loaded);
                Ok(())
            }
            Err(e) => {
                let mut state = self.state.lock().await;
                *state = EngineState::Failed(e.clone());
                Err(e)
            }
        }
    }

    pub async fn generate(&self, prompt: &str, params: GenerateParams) -> Result<String, String> {
        let state = self.state.lock().await;
        let loaded = match &*state {
            EngineState::Ready(m) => m,
            _ => return Err("AI engine not ready".into()),
        };

        let backend = loaded.backend.clone();
        let model = loaded.model.clone();
        let prompt = prompt.to_string();
        drop(state);

        tokio::task::spawn_blocking(move || {
            run_inference(&backend, &model, &prompt, &params)
        })
        .await
        .map_err(|e| format!("Spawn error: {e}"))?
    }
}

fn run_inference(
    backend: &LlamaBackend,
    model: &LlamaModel,
    prompt: &str,
    params: &GenerateParams,
) -> Result<String, String> {
    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4);

    let n_batch: usize = 512;

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(4096))
        .with_n_batch(n_batch as u32)
        .with_n_threads(n_threads)
        .with_n_threads_batch(n_threads);

    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| format!("Context creation failed: {e}"))?;

    let tokens = model
        .str_to_token(prompt, AddBos::Always)
        .map_err(|e| format!("Tokenization failed: {e}"))?;

    if tokens.len() > 3500 {
        return Err(format!("Prompt too long ({} tokens, max 3500)", tokens.len()));
    }

    // Process prompt in chunks of n_batch
    let mut batch = LlamaBatch::new(n_batch, 1);
    for (chunk_idx, chunk) in tokens.chunks(n_batch).enumerate() {
        batch.clear();
        let base_pos = (chunk_idx * n_batch) as i32;
        let is_last_chunk = chunk_idx == tokens.len() / n_batch;
        for (i, &token) in chunk.iter().enumerate() {
            let is_last_token = is_last_chunk && (base_pos + i as i32) == (tokens.len() as i32 - 1);
            batch
                .add(token, base_pos + i as i32, &[0], is_last_token)
                .map_err(|e| format!("Batch add failed: {e}"))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| format!("Prompt decode failed: {e}"))?;
    }

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::min_p(0.05, 1),
        LlamaSampler::top_p(params.top_p, 1),
        LlamaSampler::temp(params.temperature),
        LlamaSampler::dist(42),
    ]);

    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut output = String::new();
    let mut n_cur = tokens.len() as i32;

    for _ in 0..params.max_tokens {
        let token = sampler.sample(&ctx, -1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            break;
        }

        let piece = model
            .token_to_piece(token, &mut decoder, false, None)
            .map_err(|e| format!("Detokenize failed: {e}"))?;

        let combined = format!("{}{}", output, piece);
        for stop in &params.stop_sequences {
            if combined.contains(stop.as_str()) {
                let trimmed = combined.split(stop.as_str()).next().unwrap_or(&combined);
                return Ok(trimmed.to_string());
            }
        }

        output.push_str(&piece);

        batch.clear();
        batch
            .add(token, n_cur, &[0], true)
            .map_err(|e| format!("Batch add failed: {e}"))?;

        ctx.decode(&mut batch)
            .map_err(|e| format!("Decode failed: {e}"))?;

        n_cur += 1;
    }

    Ok(output)
}
