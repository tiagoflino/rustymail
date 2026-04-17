use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

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

pub struct LlmEngine {
    state: Arc<Mutex<EngineState>>,
    models_dir: PathBuf,
}

enum EngineState {
    Idle,
    Loaded {
        model_id: String,
        model_size_bytes: u64,
    },
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
            EngineState::Loaded { model_id, model_size_bytes } => EngineStatus::Ready {
                model_id: model_id.clone(),
                model_size_bytes: *model_size_bytes,
            },
        }
    }

    pub async fn load_model(&self, model_filename: &str) -> Result<(), String> {
        let path = self.models_dir.join(model_filename);
        if !path.exists() {
            return Err(format!("Model file not found: {}", path.display()));
        }

        let model_size = std::fs::metadata(&path)
            .map(|m| m.len())
            .unwrap_or(0);

        // TODO: Replace with actual llama-cpp-2 model loading in Task 3
        // For now, mark as loaded to validate the architecture end-to-end
        let mut state = self.state.lock().await;
        *state = EngineState::Loaded {
            model_id: model_filename.to_string(),
            model_size_bytes: model_size,
        };

        Ok(())
    }

    pub async fn unload(&self) {
        let mut state = self.state.lock().await;
        *state = EngineState::Idle;
    }

    pub async fn generate(&self, _prompt: &str, _params: GenerateParams) -> Result<String, String> {
        let state = self.state.lock().await;
        match &*state {
            EngineState::Idle => Err("No model loaded".into()),
            EngineState::Loaded { .. } => {
                // TODO: Replace with actual llama-cpp-2 inference in Task 3
                // Will use tokio::task::spawn_blocking for CPU-bound inference
                Ok("Summary placeholder — LLM inference will be wired in the next task.".into())
            }
        }
    }
}
