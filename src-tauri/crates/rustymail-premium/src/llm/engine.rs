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

#[derive(Debug, Clone, serde::Serialize)]
pub enum EngineStatus {
    Uninitialized,
    Loading { model_id: String },
    Ready { model_id: String, model_size_bytes: u64 },
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
    model_id: String,
    model_size_bytes: u64,
}

pub struct LlmEngine {
    state: Arc<Mutex<EngineState>>,
    models_dir: PathBuf,
}

enum EngineState {
    Idle,
    Loaded(LoadedModel),
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

    pub fn models_dir(&self) -> &PathBuf {
        &self.models_dir
    }

    pub async fn get_status(&self) -> EngineStatus {
        let state = self.state.lock().await;
        match &*state {
            EngineState::Idle => EngineStatus::Uninitialized,
            EngineState::Loaded(m) => EngineStatus::Ready {
                model_id: m.model_id.clone(),
                model_size_bytes: m.model_size_bytes,
            },
        }
    }

    pub async fn load_model(&self, model_filename: &str) -> Result<(), String> {
        let path = self.models_dir.join(model_filename);
        if !path.exists() {
            return Err(format!("Model file not found: {}", path.display()));
        }

        let model_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let model_id = model_filename.to_string();
        let path_clone = path.clone();

        let (backend, model) = tokio::task::spawn_blocking(move || -> Result<_, String> {
            let mut backend = LlamaBackend::init()
                .map_err(|e| format!("Backend init failed: {e}"))?;
            backend.void_logs();
            let model_params = LlamaModelParams::default().with_n_gpu_layers(99);
            let model = LlamaModel::load_from_file(&backend, &path_clone, &model_params)
                .map_err(|e| format!("Model load failed: {e}"))?;
            Ok((backend, model))
        })
        .await
        .map_err(|e| format!("Spawn error: {e}"))??;

        let mut state = self.state.lock().await;
        *state = EngineState::Loaded(LoadedModel {
            backend: Arc::new(backend),
            model: Arc::new(model),
            model_id,
            model_size_bytes: model_size,
        });

        Ok(())
    }

    pub async fn unload(&self) {
        let mut state = self.state.lock().await;
        *state = EngineState::Idle;
    }

    pub async fn generate(&self, prompt: &str, params: GenerateParams) -> Result<String, String> {
        let state = self.state.lock().await;
        let loaded = match &*state {
            EngineState::Idle => return Err("No model loaded".into()),
            EngineState::Loaded(m) => m,
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

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(2048))
        .with_n_batch(512)
        .with_n_threads(n_threads)
        .with_n_threads_batch(n_threads);

    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| format!("Context creation failed: {e}"))?;

    let tokens = model
        .str_to_token(prompt, AddBos::Always)
        .map_err(|e| format!("Tokenization failed: {e}"))?;

    let mut batch = LlamaBatch::new(512, 1);
    let last_idx = tokens.len() as i32 - 1;
    for (i, &token) in tokens.iter().enumerate() {
        batch
            .add(token, i as i32, &[0], i as i32 == last_idx)
            .map_err(|e| format!("Batch add failed: {e}"))?;
    }

    ctx.decode(&mut batch)
        .map_err(|e| format!("Initial decode failed: {e}"))?;

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
