//! Supervised command text adapter process lifecycle.

use vinput_text::{
    AdapterProcessSpec, AdapterRuntimePaths, AdapterStopOutcome, start_adapter_process,
    stop_adapter_process,
};

use super::{RuntimeError, RuntimeState};

impl RuntimeState {
    /// Overrides adapter runtime paths for tests or embedded callers.
    #[must_use]
    pub fn with_adapter_runtime_paths(mut self, paths: AdapterRuntimePaths) -> Self {
        self.adapter_runtime_paths = paths;
        self
    }

    /// Reaps supervised text adapters that have already exited.
    pub fn refresh_text_adapters(&mut self) -> Vec<String> {
        let exited_adapter_ids: Vec<_> = self
            .adapter_processes
            .iter_mut()
            .filter_map(|(adapter_id, process)| match process.child.try_wait() {
                Ok(Some(_status)) => Some(adapter_id.clone()),
                Ok(None) | Err(_) => None,
            })
            .collect();
        for adapter_id in &exited_adapter_ids {
            self.adapter_processes.remove(adapter_id);
            let _ = self.adapter_runtime_paths.remove_pid(adapter_id);
        }
        exited_adapter_ids
    }

    /// Starts a configured command text adapter process.
    pub fn start_text_adapter(&mut self, adapter_id: &str) -> Result<u32, RuntimeError> {
        if self.adapter_processes.contains_key(adapter_id) {
            return Err(RuntimeError::TextAdapterAlreadyRunning(
                adapter_id.to_owned(),
            ));
        }
        let adapter = self
            .config
            .llm
            .adapters
            .iter()
            .find(|adapter| adapter.id == adapter_id)
            .ok_or_else(|| RuntimeError::TextAdapterNotConfigured(adapter_id.to_owned()))?;
        let spec = AdapterProcessSpec::from_config(adapter);
        let process = start_adapter_process(&spec, &self.adapter_runtime_paths)
            .map_err(RuntimeError::TextAdapterSupervisor)?;
        let pid = process.pid;
        self.adapter_processes
            .insert(adapter_id.to_owned(), process);
        Ok(pid)
    }

    /// Stops a configured command text adapter process.
    pub fn stop_text_adapter(
        &mut self,
        adapter_id: &str,
    ) -> Result<AdapterStopOutcome, RuntimeError> {
        if !self
            .configured_text_adapters()
            .contains_command_adapter(adapter_id)
        {
            return Err(RuntimeError::TextAdapterNotConfigured(
                adapter_id.to_owned(),
            ));
        }
        let outcome = stop_adapter_process(adapter_id, &self.adapter_runtime_paths)
            .map_err(RuntimeError::TextAdapterSupervisor)?;
        if let Some(mut process) = self.adapter_processes.remove(adapter_id) {
            if matches!(outcome, AdapterStopOutcome::NotRunning) {
                let _ = process.child.kill();
            }
            let _ = process.child.wait();
        }
        Ok(outcome)
    }
}
