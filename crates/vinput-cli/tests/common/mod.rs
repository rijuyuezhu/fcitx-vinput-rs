use std::{fs, path::PathBuf, process::Command};

pub fn workspace_file(path: &str) -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.push("../..");
    root.push(path);
    root
}

#[allow(dead_code)]
pub fn workspace_crate_names() -> Vec<String> {
    let mut crates = std::fs::read_dir(workspace_file("crates"))
        .expect("read crates directory")
        .map(|entry| entry.expect("read crate directory entry").path())
        .filter(|path| path.is_dir())
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    crates.sort();
    assert!(!crates.is_empty(), "workspace crates should exist");
    crates
}

#[allow(dead_code)]
pub fn write_temp_json(prefix: &str, contents: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{prefix}-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::write(&path, contents).expect("write temporary JSON fixture");
    path
}

#[allow(dead_code)]
pub fn vinput_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vinput"))
}
