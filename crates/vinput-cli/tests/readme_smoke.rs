//! Regression tests that keep README smoke commands aligned with the justfile.

use std::path::PathBuf;

fn workspace_file(path: &str) -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.push("../..");
    root.push(path);
    root
}

fn just_smoke_commands(justfile: &str) -> Vec<&str> {
    let mut lines = justfile.lines().skip_while(|line| *line != "smoke:");
    assert_eq!(lines.next(), Some("smoke:"));

    lines
        .take_while(|line| line.is_empty() || line.starts_with("    "))
        .filter_map(|line| line.strip_prefix("    "))
        .filter(|line| !line.is_empty())
        .collect()
}

#[test]
fn readme_lists_just_smoke_commands_in_order() {
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");
    let readme = std::fs::read_to_string(workspace_file("README.md")).expect("read README");
    let smoke_commands = just_smoke_commands(&justfile);

    assert!(
        !smoke_commands.is_empty(),
        "justfile should define smoke commands"
    );

    let mut search_from = 0;
    for command in smoke_commands {
        let offset = readme[search_from..]
            .find(command)
            .unwrap_or_else(|| panic!("README should list smoke command `{command}`"));
        search_from += offset + command.len();
    }
}
