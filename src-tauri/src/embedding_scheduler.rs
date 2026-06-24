//! Opportunistic embedding scheduler with resource guardrails.
//!
//! Three modes:
//! 1. OPPORTUNISTIC: model already warm from user interaction, process pending
//! 2. POST_SYNC: just synced new mail, process if resources allow
//! 3. MAINTENANCE: user explicitly requested backfill, on AC power
//!
//! Resource guardrails:
//! - Battery threshold (default: 50%, don't process below)
//! - AC power preferred for backfill
//! - Cooldown between batches (30s default)
//! - Max total indexed (5000 default)
//! - Batch size (20 default)
//!
//! The decision logic and power detection are exercised by unit tests; the
//! live loop (`spawn_scheduler`) only exists on the premium build, since it
//! needs the local LLM engine to produce embeddings. On a non-premium,
//! non-test build the machinery is intentionally inert.
#![cfg_attr(not(feature = "premium"), allow(dead_code))]

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
    #[cfg_attr(not(feature = "premium"), allow(dead_code))]
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

#[cfg_attr(not(feature = "premium"), allow(dead_code))]
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

    #[allow(dead_code)]
    pub fn is_processing(&self) -> bool {
        self.is_processing
    }

    pub fn config(&self) -> &EmbeddingConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut EmbeddingConfig {
        &mut self.config
    }

    #[allow(dead_code)]
    pub fn remaining_capacity(&self) -> usize {
        self.config.max_messages.saturating_sub(self.total_processed)
    }
}

/// Snapshot of host power state used by the scheduler guardrails.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerState {
    pub on_ac_power: bool,
    /// Fraction in [0.0, 1.0]. 1.0 when on AC with no battery info.
    pub battery_pct: f32,
}

impl Default for PowerState {
    fn default() -> Self {
        // Safe default: assume plugged in and full (do not block on unknown).
        Self { on_ac_power: true, battery_pct: 1.0 }
    }
}

/// Read host power state via `starship-battery` (the maintained successor to
/// the `battery` crate). Degrades safely to "on AC, full" on any platform
/// without battery support or on error — we never want a detection failure to
/// permanently stall indexing.
///
/// A machine is considered "on AC" when it has no batteries (desktop/server)
/// or when no battery is actively discharging.
pub fn read_power_state() -> PowerState {
    use starship_battery::units::ratio::percent;
    use starship_battery::{Manager, State};

    let manager = match Manager::new() {
        Ok(m) => m,
        Err(_) => return PowerState::default(),
    };
    let batteries = match manager.batteries() {
        Ok(b) => b,
        Err(_) => return PowerState::default(),
    };

    let mut any_discharging = false;
    let mut any_battery = false;
    let mut min_pct: f32 = 1.0;
    for maybe_bat in batteries {
        let bat = match maybe_bat {
            Ok(b) => b,
            Err(_) => continue,
        };
        any_battery = true;
        let pct = (bat.state_of_charge().get::<percent>() / 100.0).clamp(0.0, 1.0);
        if pct < min_pct {
            min_pct = pct;
        }
        if matches!(bat.state(), State::Discharging) {
            any_discharging = true;
        }
    }

    if !any_battery {
        return PowerState::default();
    }
    PowerState {
        on_ac_power: !any_discharging,
        battery_pct: min_pct,
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

/// Spawn the background opportunistic indexing loop. Premium-only: it needs
/// the local LLM engine to produce embeddings. The loop wakes periodically,
/// reads host power state, consults `should_process`, and runs a batch when the
/// guardrails allow. Runs on the active account.
#[cfg(feature = "premium")]
pub fn spawn_scheduler(app_handle: tauri::AppHandle, pool: sqlx::SqlitePool) {
    use tauri::Manager;

    tauri::async_runtime::spawn(async move {
        // Repair any flag/table divergence left by a previous crash before we
        // start, so status and search agree.
        if let Err(e) = crate::semantic_search::backfill_embedded_flag(&pool).await {
            tracing::warn!("embedding backfill reconcile failed: {e}");
        }

        let tick = std::time::Duration::from_secs(60);
        let mut scheduler = EmbeddingScheduler::new(EmbeddingConfig::from_db(&pool).await);

        loop {
            tokio::time::sleep(tick).await;

            // Refresh config each tick so settings changes take effect live.
            let config = EmbeddingConfig::from_db(&pool).await;
            if !config.auto_index {
                continue;
            }
            // Rebuild only if the config changed in a way that matters.
            *scheduler.config_mut() = config;

            let account_id = match crate::commands::accounts::get_active_account(&pool).await {
                Ok(a) => a.id,
                Err(_) => continue, // No active account yet.
            };

            // Nothing pending => skip cheaply.
            match crate::semantic_search::count_pending(&pool, &account_id).await {
                Ok(0) | Err(_) => continue,
                Ok(_) => {}
            }

            let engine = app_handle.state::<rustymail_premium::llm::engine::LlmEngine>();
            let model_loaded = matches!(
                engine.get_status().await,
                rustymail_premium::llm::engine::AiStatus::Ready { .. }
            );
            let power = read_power_state();

            let decision = scheduler.should_process(model_loaded, power.on_ac_power, power.battery_pct);
            if !decision.should_process() {
                tracing::debug!("embedding scheduler skip: {}", decision.reason());
                continue;
            }

            // Cold start needs the model; warm it only when guards already
            // approved loading it.
            if !model_loaded {
                if let Err(e) = engine.ensure_ready().await {
                    tracing::warn!("embedding scheduler: engine not ready: {e}");
                    continue;
                }
            }

            scheduler.mark_batch_start();
            let batch_size = scheduler.config().batch_size as i64;
            let processed = match crate::commands::semantic::run_embedding_batch(
                &pool, &engine, &account_id, batch_size,
            )
            .await
            {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!("embedding batch failed: {e}");
                    0
                }
            };
            scheduler.mark_batch_done(processed);
            if processed > 0 {
                tracing::info!("embedded {processed} message(s)");
            }
        }
    });
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

    #[test]
    fn test_read_power_state_never_panics_and_is_sane() {
        // On CI/desktop with no battery this returns the safe default; on a
        // laptop it returns real values. Either way it must be in-range and
        // must not panic.
        let ps = read_power_state();
        assert!(ps.battery_pct >= 0.0 && ps.battery_pct <= 1.0);
    }

    #[test]
    fn test_power_state_default_is_ac_full() {
        let ps = PowerState::default();
        assert!(ps.on_ac_power);
        assert_eq!(ps.battery_pct, 1.0);
    }

    /// Integration-style test of the run-loop decision: a fresh scheduler with
    /// pending work, model cold, on battery with auto-index off must refuse;
    /// once on AC it must proceed; after a batch it must cool down.
    #[test]
    fn test_run_loop_decision_sequence() {
        let mut scheduler = EmbeddingScheduler::new(EmbeddingConfig::default());

        // Cold + on battery (discharging) + index_on_battery=false => refuse.
        let on_battery = PowerState { on_ac_power: false, battery_pct: 0.8 };
        let d1 = scheduler.should_process(false, on_battery.on_ac_power, on_battery.battery_pct);
        assert!(!d1.should_process(), "Should refuse on battery when index_on_battery=false");

        // Plug into AC => proceed.
        let on_ac = PowerState::default();
        let d2 = scheduler.should_process(false, on_ac.on_ac_power, on_ac.battery_pct);
        assert!(d2.should_process(), "Should proceed on AC power");

        // Simulate running the batch.
        scheduler.mark_batch_start();
        assert!(scheduler.is_processing());
        scheduler.mark_batch_done(20);
        assert!(!scheduler.is_processing());

        // Immediately after => cooldown even on AC.
        let d3 = scheduler.should_process(false, on_ac.on_ac_power, on_ac.battery_pct);
        assert!(matches!(d3, ProcessDecision::Cooldown { .. }), "Should cooldown right after a batch");
    }
}
