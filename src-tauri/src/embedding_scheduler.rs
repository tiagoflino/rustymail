/// Opportunistic embedding scheduler with resource guardrails.
///
/// Three modes:
/// 1. OPPORTUNISTIC: model already warm from user interaction, process pending
/// 2. POST_SYNC: just synced new mail, process if resources allow
/// 3. MAINTENANCE: user explicitly requested backfill, on AC power
///
/// Resource guardrails:
/// - Battery threshold (default: 50%, don't process below)
/// - AC power preferred for backfill
/// - Cooldown between batches (30s default)
/// - Max total indexed (5000 default)
/// - Batch size (20 default)

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub auto_index: bool,
    pub batch_size: usize,
    pub max_messages: usize,
    pub index_on_battery: bool,
    pub min_battery_pct: f32,
    pub cooldown_secs: u64,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            auto_index: true,
            batch_size: 20,
            max_messages: 5000,
            index_on_battery: false,
            min_battery_pct: 0.50,
            cooldown_secs: 30,
        }
    }
}

impl EmbeddingConfig {
    pub async fn from_db(pool: &SqlitePool) -> Self {
        let mut config = Self::default();
        if let Ok(v) = get_setting(pool, "embedding_auto_index").await {
            config.auto_index = v == "true";
        }
        if let Ok(v) = get_setting(pool, "embedding_batch_size").await {
            if let Ok(n) = v.parse::<usize>() { config.batch_size = n; }
        }
        if let Ok(v) = get_setting(pool, "embedding_max_messages").await {
            if let Ok(n) = v.parse::<usize>() { config.max_messages = n; }
        }
        if let Ok(v) = get_setting(pool, "embedding_index_on_battery").await {
            config.index_on_battery = v == "true";
        }
        config
    }
}

async fn get_setting(pool: &SqlitePool, key: &str) -> Result<String, ()> {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|_| ())?
        .ok_or(())
}

#[derive(Debug)]
pub struct EmbeddingScheduler {
    config: EmbeddingConfig,
    last_batch_time: Option<std::time::Instant>,
    is_processing: bool,
    total_processed: usize,
}

impl EmbeddingScheduler {
    pub fn new(config: EmbeddingConfig) -> Self {
        Self {
            config,
            last_batch_time: None,
            is_processing: false,
            total_processed: 0,
        }
    }

    /// Check if we should process a batch right now.
    /// Model-loaded always wins — it's a free ride with zero incremental cost.
    /// Battery/AC guards only apply when we'd need to load the model cold.
    pub fn should_process(&self, model_loaded: bool, on_ac_power: bool, battery_pct: f32) -> ProcessDecision {
        if self.is_processing {
            return ProcessDecision::AlreadyProcessing;
        }

        // Cap check (applies in all modes)
        if self.total_processed >= self.config.max_messages {
            return ProcessDecision::CapReached(self.config.max_messages);
        }

        // MODEL LOADED: free ride — skip battery/AC guards, respect only cooldown
        if model_loaded {
            if let Some(last) = self.last_batch_time {
                if last.elapsed().as_secs() < self.config.cooldown_secs {
                    return ProcessDecision::Cooldown {
                        remaining_secs: self.config.cooldown_secs - last.elapsed().as_secs(),
                    };
                }
            }
            return ProcessDecision::Process { reason: "Model already loaded (opportunistic)".into() };
        }

        // COLD START: apply resource guards before loading the model
        if let Some(last) = self.last_batch_time {
            if last.elapsed().as_secs() < self.config.cooldown_secs {
                return ProcessDecision::Cooldown {
                    remaining_secs: self.config.cooldown_secs - last.elapsed().as_secs(),
                };
            }
        }

        if !on_ac_power {
            if !self.config.index_on_battery {
                return ProcessDecision::NeedsAcPower;
            }
            if battery_pct < self.config.min_battery_pct {
                return ProcessDecision::BatteryTooLow {
                    current: battery_pct,
                    required: self.config.min_battery_pct,
                };
            }
        }

        if on_ac_power {
            ProcessDecision::Process { reason: "On AC power".into() }
        } else if self.config.auto_index {
            ProcessDecision::Process { reason: "Auto-index enabled".into() }
        } else {
            ProcessDecision::Skipped("Auto-index disabled".into())
        }
    }

    pub fn mark_batch_start(&mut self) {
        self.is_processing = true;
    }

    pub fn mark_batch_done(&mut self, processed: usize) {
        self.is_processing = false;
        self.last_batch_time = Some(std::time::Instant::now());
        self.total_processed += processed;
    }

    pub fn is_processing(&self) -> bool {
        self.is_processing
    }

    pub fn config(&self) -> &EmbeddingConfig {
        &self.config
    }

    pub fn remaining_capacity(&self) -> usize {
        self.config.max_messages.saturating_sub(self.total_processed)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessDecision {
    Process { reason: String },
    AlreadyProcessing,
    Cooldown { remaining_secs: u64 },
    NeedsAcPower,
    BatteryTooLow { current: f32, required: f32 },
    CapReached(usize),
    Skipped(String),
}

impl ProcessDecision {
    pub fn should_process(&self) -> bool {
        matches!(self, ProcessDecision::Process { .. })
    }

    pub fn reason(&self) -> String {
        match self {
            ProcessDecision::Process { reason } => reason.clone(),
            ProcessDecision::AlreadyProcessing => "Already processing a batch".into(),
            ProcessDecision::Cooldown { remaining_secs } => format!("Cooldown: {}s remaining", remaining_secs),
            ProcessDecision::NeedsAcPower => "AC power required for backfill".into(),
            ProcessDecision::BatteryTooLow { current, required } => format!("Battery {:.0}% below {:.0}% threshold", current * 100.0, required * 100.0),
            ProcessDecision::CapReached(max) => format!("Cap reached: {max} messages indexed"),
            ProcessDecision::Skipped(msg) => msg.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EmbeddingConfig::default();
        assert!(config.auto_index);
        assert_eq!(config.batch_size, 20);
        assert_eq!(config.max_messages, 5000);
        assert!(!config.index_on_battery);
        assert_eq!(config.min_battery_pct, 0.50);
        assert_eq!(config.cooldown_secs, 30);
    }

    #[test]
    fn test_should_process_model_loaded() {
        let scheduler = EmbeddingScheduler::new(EmbeddingConfig::default());
        let decision = scheduler.should_process(true, false, 1.0);
        assert!(decision.should_process());
    }

    #[test]
    fn test_should_process_ac_power() {
        let scheduler = EmbeddingScheduler::new(EmbeddingConfig::default());
        let decision = scheduler.should_process(false, true, 0.3);
        assert!(decision.should_process());
    }

    #[test]
    fn test_should_skip_low_battery() {
        // With index_on_battery=false: NeedsAcPower
        let scheduler = EmbeddingScheduler::new(EmbeddingConfig::default());
        let decision = scheduler.should_process(false, false, 0.3);
        assert!(!decision.should_process());
        assert!(matches!(decision, ProcessDecision::NeedsAcPower));
    }

    #[test]
    fn test_battery_too_low_when_index_on_battery_enabled() {
        let mut config = EmbeddingConfig::default();
        config.index_on_battery = true;
        let scheduler = EmbeddingScheduler::new(config);
        let decision = scheduler.should_process(false, false, 0.3);
        assert!(!decision.should_process());
        assert!(matches!(decision, ProcessDecision::BatteryTooLow { .. }));
    }

    #[test]
    fn test_battery_index_enabled_allows_battery() {
        let mut config = EmbeddingConfig::default();
        config.index_on_battery = true;
        let scheduler = EmbeddingScheduler::new(config);
        let decision = scheduler.should_process(false, false, 0.8);
        assert!(decision.should_process());
    }

    #[test]
    fn test_cooldown_respected() {
        let mut scheduler = EmbeddingScheduler::new(EmbeddingConfig::default());
        scheduler.mark_batch_done(20);
        let decision = scheduler.should_process(true, true, 1.0);
        assert!(!decision.should_process());
        assert!(matches!(decision, ProcessDecision::Cooldown { .. }));
    }

    #[test]
    fn test_cap_reached() {
        let mut config = EmbeddingConfig::default();
        config.max_messages = 10;
        let mut scheduler = EmbeddingScheduler::new(config);
        scheduler.mark_batch_done(10);
        let decision = scheduler.should_process(true, true, 1.0);
        assert!(!decision.should_process());
        assert!(matches!(decision, ProcessDecision::CapReached(10)));
    }

    #[test]
    fn test_remaining_capacity() {
        let mut config = EmbeddingConfig::default();
        config.max_messages = 100;
        let mut scheduler = EmbeddingScheduler::new(config);
        scheduler.mark_batch_done(30);
        assert_eq!(scheduler.remaining_capacity(), 70);
    }

    #[test]
    fn test_process_decision_reason_strings() {
        assert!(ProcessDecision::Process { reason: "test".into() }.reason().contains("test"));
        assert!(ProcessDecision::AlreadyProcessing.reason().contains("Already processing"));
        assert!(ProcessDecision::NeedsAcPower.reason().contains("AC power"));
    }
}
