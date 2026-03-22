use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Lifecycle state for a service webview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceLifecycleState {
    /// Fully visible and rendering, JS running, WebSockets alive.
    Active,
    /// Hidden (`was_hidden(1)`), JS throttled by Chromium, WebSockets alive.
    /// Polling interval extended to 5s.
    Backgrounded,
    /// Heavily throttled, audio muted, no polling. WebSockets still alive.
    Frozen,
    /// Browser destroyed, no resources consumed.
    Hibernated,
}

impl Default for ServiceLifecycleState {
    fn default() -> Self {
        Self::Active
    }
}

/// Runtime state for a service (not persisted).
pub struct ServiceRuntimeState {
    pub lifecycle: ServiceLifecycleState,
    pub last_active: Instant,
    pub direct_count: u32,
    pub indirect_count: u32,
    pub dialog_title: Option<String>,
}

impl ServiceRuntimeState {
    pub fn new() -> Self {
        Self {
            lifecycle: ServiceLifecycleState::Active,
            last_active: Instant::now(),
            direct_count: 0,
            indirect_count: 0,
            dialog_title: None,
        }
    }

    pub fn total_unread(&self) -> u32 {
        self.direct_count + self.indirect_count
    }

    pub fn touch(&mut self) {
        self.last_active = Instant::now();
    }
}
