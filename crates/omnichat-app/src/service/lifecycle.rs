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

/// Schedule the first lifecycle tick. Subsequent ticks reschedule themselves.
/// Uses the same (env-overridable) cadence as recurring ticks, so the initial
/// delay also honors OMNICHAT_TICK_MS.
pub fn schedule(state: SharedState) {
    let mut task = LifecycleTickTask::new(state);
    post_delayed_task(ThreadId::UI, Some(&mut task), tick_ms());
}

/// Manages lifecycle transitions for service browsers.
/// Called periodically from a CEF timer task.
pub struct LifecycleManager;

impl LifecycleManager {
    /// Check all services and transition as needed.
    /// Should be called on the CEF UI thread.
    pub fn tick(state: &SharedState) {
        let now = Instant::now();
        let freeze_after = freeze_after();
        let hibernate_after = hibernate_after();

        // Phase 1 — decide transitions under the lock; collect browser handles +
        // pending state updates; then DROP the lock. We must NOT hold the state
        // lock during CEF host ops (close_browser triggers on_before_close, which
        // re-locks state): same discipline as swap_content_view.
        let mut to_freeze: Vec<Browser> = Vec::new();
        let mut to_hibernate: Vec<Browser> = Vec::new();
        let mut updates: Vec<(String, ServiceLifecycleState)> = Vec::new();
        {
            let s = state.lock();
            let active_id = s.active_service_id.clone();
            let service_ids: Vec<String> = s
                .service_manager
                .services()
                .iter()
                .filter(|svc| svc.is_enabled && svc.is_hibernation_enabled)
                .map(|svc| svc.id.clone())
                .collect();

            for id in service_ids {
                if active_id.as_ref() == Some(&id) {
                    continue;
                }
                let (current_state, idle_time) = match s.service_manager.get_runtime(&id) {
                    Some(rt) => (rt.lifecycle, now.duration_since(rt.last_active)),
                    None => continue,
                };
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
                info!("Service {id}: {current_state:?} → {new_state:?} (idle {idle_time:?})");
                match new_state {
                    ServiceLifecycleState::Frozen => {
                        if let Some(b) = s.browsers.get(&id) {
                            to_freeze.push(b.clone());
                        }
                    }
                    ServiceLifecycleState::Hibernated => {
                        if let Some(b) = s.browsers.get(&id) {
                            to_hibernate.push(b.clone());
                        }
                    }
                    _ => {}
                }
                updates.push((id, new_state));
            }
        }

        // Phase 2 — CEF host ops without the lock held.
        for b in &to_freeze {
            if let Some(host) = b.host() {
                host.set_audio_muted(1);
            }
        }
        for b in &to_hibernate {
            // Discard the page to about:blank: this frees the page's DOM/JS heap
            // (the bulk of a service's RAM) while keeping the browser + view alive
            // (no dead pane; switching stays instant). Reactivation reloads the
            // real URL and the persistent profile restores the session.
            // close_browser does NOT work here — it cannot tear down a
            // BrowserView-backed browser, so it reclaims nothing.
            // Discard via a RENDERER-initiated navigation (execute_java_script),
            // NOT frame.load_url(): load_url calls CEF's RequestFocusSync on the
            // BrowserView, which dereferences the view's Widget. A hibernated
            // service is backgrounded — its view is detached from the window — so
            // that Widget is gone and load_url segfaults (browser_view_impl.cc
            // RequestFocusSync -> Widget::IsMinimized on a dead weak_ptr). A JS
            // location.replace runs in the still-alive renderer and frees the
            // page heap without touching host-side focus.
            if let Some(frame) = b.main_frame() {
                let js = CefString::from("location.replace('about:blank')");
                let url = CefString::from("omnichat://hibernate");
                frame.execute_java_script(Some(&js), Some(&url), 0);
            }
        }

        // Phase 3 — record the new lifecycle states.
        if !updates.is_empty() {
            let mut s = state.lock();
            for (id, ns) in updates {
                s.service_manager.set_lifecycle_state(&id, ns);
            }
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
