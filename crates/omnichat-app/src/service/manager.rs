use log::info;
use std::collections::HashMap;

use super::config::ServiceConfig;
use super::state::{ServiceLifecycleState, ServiceRuntimeState};

/// Manages the set of configured services and their runtime state.
pub struct ServiceManager {
    services: Vec<ServiceConfig>,
    runtime: HashMap<String, ServiceRuntimeState>,
}

impl ServiceManager {
    pub fn new(services: Vec<ServiceConfig>) -> Self {
        let mut runtime = HashMap::new();
        for svc in &services {
            runtime.insert(svc.id.clone(), ServiceRuntimeState::new());
        }
        Self { services, runtime }
    }

    pub fn services(&self) -> &Vec<ServiceConfig> {
        &self.services
    }

    pub fn add_service(&mut self, config: ServiceConfig) {
        info!("Adding service: {} ({})", config.name, config.id);
        self.runtime
            .insert(config.id.clone(), ServiceRuntimeState::new());
        self.services.push(config);
    }

    pub fn remove_service(&mut self, id: &str) {
        self.services.retain(|s| s.id != id);
        self.runtime.remove(id);
        info!("Removed service: {id}");
    }

    pub fn get_config(&self, id: &str) -> Option<&ServiceConfig> {
        self.services.iter().find(|s| s.id == id)
    }

    pub fn get_config_mut(&mut self, id: &str) -> Option<&mut ServiceConfig> {
        self.services.iter_mut().find(|s| s.id == id)
    }

    pub fn get_runtime(&self, id: &str) -> Option<&ServiceRuntimeState> {
        self.runtime.get(id)
    }

    pub fn get_runtime_mut(&mut self, id: &str) -> Option<&mut ServiceRuntimeState> {
        self.runtime.get_mut(id)
    }

    pub fn update_badge(&mut self, service_id: &str, direct: u32, indirect: u32) {
        if let Some(rt) = self.runtime.get_mut(service_id) {
            rt.direct_count = direct;
            rt.indirect_count = indirect;
            rt.touch();
        }
    }

    pub fn set_dialog_title(&mut self, service_id: &str, title: Option<String>) {
        if let Some(rt) = self.runtime.get_mut(service_id) {
            rt.dialog_title = title;
        }
    }

    pub fn set_lifecycle_state(&mut self, service_id: &str, state: ServiceLifecycleState) {
        if let Some(rt) = self.runtime.get_mut(service_id) {
            rt.lifecycle = state;
        }
    }

    pub fn total_direct_count(&self) -> u32 {
        self.runtime.values().map(|r| r.direct_count).sum()
    }

    pub fn total_indirect_count(&self) -> u32 {
        self.runtime.values().map(|r| r.indirect_count).sum()
    }

    pub fn total_unread(&self) -> u32 {
        self.total_direct_count() + self.total_indirect_count()
    }

    /// Returns services sorted by sort_order.
    pub fn sorted_services(&self) -> Vec<&ServiceConfig> {
        let mut sorted: Vec<&ServiceConfig> = self.services.iter().collect();
        sorted.sort_by_key(|s| s.sort_order);
        sorted
    }
}
