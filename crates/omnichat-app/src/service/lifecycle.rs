use cef::*;
use log::info;
use std::time::{Duration, Instant};

use super::state::ServiceLifecycleState;
use crate::app::SharedState;

/// Timeout thresholds for lifecycle transitions (seconds), env-overridable so
/// hibernation/freeze can be exercised in seconds during testing/perf work.
/// Defaults preserve current behavior: freeze after 5 min, hibernate after 15 min.
fn env_secs(key: &str, default: u64) -> Duration {
    Duration::from_secs(
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default),
    )
}
fn freeze_after() -> Duration {
    env_secs("OMNICHAT_FREEZE_SECS", 5 * 60)
}
fn hibernate_after() -> Duration {
    env_secs("OMNICHAT_HIBERNATE_SECS", 15 * 60)
}
/// Lifecycle tick cadence (ms), env-overridable. Default 30s.
fn tick_ms() -> i64 {
    std::env::var("OMNICHAT_TICK_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30_000)
}

/// Manages lifecycle transitions for service browsers.
/// Called periodically from a CEF timer task.
pub struct LifecycleManager;

impl LifecycleManager {
    /// Check all services and transition as needed.
    /// Should be called on the CEF UI thread.
    pub fn tick(state: &SharedState) {
        let mut s = state.lock();
        let now = Instant::now();
        let active_id = s.active_service_id.clone();

        let service_ids: Vec<String> = s
            .service_manager
            .services()
            .iter()
            .filter(|svc| svc.is_enabled && svc.is_hibernation_enabled)
            .map(|svc| svc.id.clone())
            .collect();

        for id in service_ids {
            // Don't touch the active service.
            if active_id.as_ref() == Some(&id) {
                continue;
            }

            let (current_state, idle_time) = {
                if let Some(rt) = s.service_manager.get_runtime(&id) {
                    (rt.lifecycle, now.duration_since(rt.last_active))
                } else {
                    continue;
                }
            };

            let freeze_after = freeze_after();
            let hibernate_after = hibernate_after();
            let new_state = match current_state {
                ServiceLifecycleState::Backgrounded if idle_time >= hibernate_after => {
                    ServiceLifecycleState::Hibernated
                }
                ServiceLifecycleState::Backgrounded if idle_time >= freeze_after => {
                    ServiceLifecycleState::Frozen
                }
                ServiceLifecycleState::Frozen if idle_time >= hibernate_after => {
                    ServiceLifecycleState::Hibernated
                }
                _ => continue,
            };

            info!(
                "Service {id}: {:?} → {:?} (idle {:?})",
                current_state, new_state, idle_time
            );

            // Apply the transition.
            match new_state {
                ServiceLifecycleState::Frozen => {
                    if let Some(browser) = s.browsers.get(&id) {
                        if let Some(host) = browser.host() {
                            host.set_audio_muted(1);
                        }
                    }
                    s.service_manager.set_lifecycle_state(&id, new_state);
                }
                ServiceLifecycleState::Hibernated => {
                    // Close the browser entirely.
                    if let Some(browser) = s.browsers.get(&id) {
                        if let Some(host) = browser.host() {
                            host.close_browser(1);
                        }
                    }
                    s.service_manager.set_lifecycle_state(&id, new_state);
                }
                _ => {}
            }
        }
    }

    /// Activate a service: bring it to Active state, background the previous one.
    pub fn activate_service(state: &SharedState, service_id: &str) {
        let mut s = state.lock();

        // Background the currently active service.
        if let Some(prev_id) = s.active_service_id.take() {
            if prev_id != service_id {
                if let Some(browser) = s.browsers.get(&prev_id) {
                    if let Some(host) = browser.host() {
                        host.was_hidden(1);
                    }
                }
                s.service_manager
                    .set_lifecycle_state(&prev_id, ServiceLifecycleState::Backgrounded);
            }
        }

        // Activate the target service.
        s.active_service_id = Some(service_id.to_string());

        if let Some(browser) = s.browsers.get(service_id) {
            if let Some(host) = browser.host() {
                host.was_hidden(0);
                host.set_audio_muted(0);
            }
        }

        s.service_manager
            .set_lifecycle_state(service_id, ServiceLifecycleState::Active);

        if let Some(rt) = s.service_manager.get_runtime_mut(service_id) {
            rt.touch();
        }
    }
}

// CEF task that runs the lifecycle manager periodically.
wrap_task! {
    pub struct LifecycleTickTask {
        state: SharedState,
    }

    impl Task {
        fn execute(&self) {
            debug_assert_ne!(currently_on(ThreadId::UI), 0);

            LifecycleManager::tick(&self.state);

            // Reschedule at the (env-overridable) tick cadence.
            let mut next = LifecycleTickTask::new(self.state.clone());
            post_delayed_task(ThreadId::UI, Some(&mut next), tick_ms());
        }
    }
}
