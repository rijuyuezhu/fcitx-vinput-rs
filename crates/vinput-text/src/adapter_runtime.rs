//! Runtime filesystem paths and process supervision for command text adapters.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use vinput_config::LlmAdapterConfig;

use crate::TextError;

/// Returns the default text adapter runtime directory.
///
/// On Linux desktop sessions this should be rooted under `XDG_RUNTIME_DIR`.
/// Tests can pass an explicit value to keep the path deterministic; production
/// callers can use [`AdapterRuntimePaths::for_current_user`].
#[must_use]
pub fn default_adapter_runtime_dir(xdg_runtime_dir: Option<&Path>) -> PathBuf {
    let base = xdg_runtime_dir
        .filter(|path| !path.as_os_str().is_empty())
        .map_or_else(std::env::temp_dir, Path::to_path_buf);
    base.join("vinput").join("adapters")
}

/// Filesystem layout helper for supervised text adapter runtime state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRuntimePaths {
    runtime_dir: PathBuf,
}

impl AdapterRuntimePaths {
    /// Creates runtime paths rooted at `runtime_dir`.
    #[must_use]
    pub fn new(runtime_dir: impl Into<PathBuf>) -> Self {
        Self {
            runtime_dir: runtime_dir.into(),
        }
    }

    /// Creates runtime paths for the current user session.
    #[must_use]
    pub fn for_current_user() -> Self {
        let xdg_runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        Self::new(default_adapter_runtime_dir(xdg_runtime_dir.as_deref()))
    }

    /// Returns the runtime directory.
    #[must_use]
    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    /// Builds a path for an adapter pid file using a safe adapter id.
    pub fn pid_path(&self, adapter_id: &str) -> Result<PathBuf, TextError> {
        Ok(self.runtime_dir.join(adapter_pid_file_name(adapter_id)?))
    }

    /// Writes an adapter pid file and returns its path.
    pub fn write_pid(&self, adapter_id: &str, pid: u32) -> Result<PathBuf, TextError> {
        let path = self.pid_path(adapter_id)?;
        fs::create_dir_all(&self.runtime_dir).map_err(|error| {
            TextError::AdapterRuntimeIo(format!(
                "failed to create adapter runtime directory `{}`: {error}",
                self.runtime_dir.display()
            ))
        })?;
        fs::write(&path, pid.to_string()).map_err(|error| {
            TextError::AdapterRuntimeIo(format!(
                "failed to write adapter pid file `{}`: {error}",
                path.display()
            ))
        })?;
        Ok(path)
    }

    /// Reads an adapter pid file. Missing files return `Ok(None)`.
    pub fn read_pid(&self, adapter_id: &str) -> Result<Option<u32>, TextError> {
        let path = self.pid_path(adapter_id)?;
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(TextError::AdapterRuntimeIo(format!(
                    "failed to read adapter pid file `{}`: {error}",
                    path.display()
                )));
            }
        };
        let trimmed = content.trim();
        trimmed.parse::<u32>().map(Some).map_err(|error| {
            TextError::InvalidAdapterPid(format!("invalid pid in `{}`: {error}", path.display()))
        })
    }

    /// Removes an adapter pid file. Missing files return `Ok(false)`.
    pub fn remove_pid(&self, adapter_id: &str) -> Result<bool, TextError> {
        let path = self.pid_path(adapter_id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(TextError::AdapterRuntimeIo(format!(
                "failed to remove adapter pid file `{}`: {error}",
                path.display()
            ))),
        }
    }
}

fn adapter_pid_file_name(adapter_id: &str) -> Result<String, TextError> {
    if adapter_id.is_empty()
        || adapter_id == "."
        || adapter_id == ".."
        || adapter_id.contains('/')
        || adapter_id.contains('\\')
    {
        return Err(TextError::InvalidAdapterId(adapter_id.to_owned()));
    }
    Ok(format!("{adapter_id}.pid"))
}

/// Command specification for a supervised text adapter process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterProcessSpec {
    /// Stable adapter id.
    pub id: String,
    /// Executable path or program name.
    pub command: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Environment variables added to the child process.
    pub env: std::collections::HashMap<String, String>,
    /// Optional child working directory.
    pub working_dir: Option<String>,
}

impl AdapterProcessSpec {
    /// Builds a process spec from typed adapter config.
    #[must_use]
    pub fn from_config(config: &LlmAdapterConfig) -> Self {
        Self {
            id: config.id.clone(),
            command: config.command.clone(),
            args: config.args.clone(),
            env: config.env.clone(),
            working_dir: config.working_dir.clone(),
        }
    }
}

/// A started adapter child process whose pid file has been written.
#[derive(Debug)]
pub struct StartedAdapterProcess {
    /// Stable adapter id.
    pub id: String,
    /// Child process id.
    pub pid: u32,
    /// Path to the written pid file.
    pub pid_path: PathBuf,
    /// Running child process handle.
    pub child: Child,
}

/// Result of asking the supervisor to stop an adapter process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterStopOutcome {
    /// No pid file existed, so no process was targeted.
    NotRunning,
    /// A TERM signal was sent and the pid file was removed.
    Stopped {
        /// Process id read from the pid file.
        pid: u32,
    },
}

/// Stops a text adapter process from its pid file and removes the pid file.
pub fn stop_adapter_process(
    adapter_id: &str,
    paths: &AdapterRuntimePaths,
) -> Result<AdapterStopOutcome, TextError> {
    let Some(pid) = paths.read_pid(adapter_id)? else {
        return Ok(AdapterStopOutcome::NotRunning);
    };
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .map_err(|error| {
            TextError::AdapterRuntimeIo(format!(
                "failed to invoke kill for text adapter `{adapter_id}` pid {pid}: {error}"
            ))
        })?;
    if !status.success() {
        return Err(TextError::AdapterRuntimeIo(format!(
            "failed to stop text adapter `{adapter_id}` pid {pid}: kill exited with {status}"
        )));
    }
    paths.remove_pid(adapter_id)?;
    Ok(AdapterStopOutcome::Stopped { pid })
}

/// Starts a text adapter process and writes its pid file.
pub fn start_adapter_process(
    spec: &AdapterProcessSpec,
    paths: &AdapterRuntimePaths,
) -> Result<StartedAdapterProcess, TextError> {
    let mut command = Command::new(&spec.command);
    command
        .args(&spec.args)
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(working_dir) = &spec.working_dir {
        command.current_dir(working_dir);
    }

    let mut child = command.spawn().map_err(|error| {
        TextError::AdapterFailed(format!(
            "failed to spawn text adapter `{}`: {error}",
            spec.id
        ))
    })?;
    let pid = child.id();
    let pid_path = match paths.write_pid(&spec.id, pid) {
        Ok(path) => path,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };

    Ok(StartedAdapterProcess {
        id: spec.id.clone(),
        pid,
        pid_path,
        child,
    })
}
