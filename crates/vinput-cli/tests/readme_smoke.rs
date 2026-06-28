//! Regression tests that keep README smoke commands aligned with the justfile.

mod common;

use common::workspace_file;

fn just_recipe_commands<'a>(justfile: &'a str, recipe: &str) -> Vec<&'a str> {
    let header = format!("{recipe}:");
    let mut lines = justfile.lines().skip_while(|line| *line != header);
    assert_eq!(lines.next(), Some(header.as_str()));

    lines
        .take_while(|line| line.is_empty() || line.starts_with("    "))
        .filter_map(|line| line.strip_prefix("    "))
        .filter(|line| !line.is_empty())
        .collect()
}

fn just_check_recipes(justfile: &str) -> Vec<&str> {
    let check_line = justfile
        .lines()
        .find(|line| line.starts_with("check:"))
        .expect("justfile should define check recipe");
    check_line
        .strip_prefix("check:")
        .expect("check recipe should have a colon")
        .split_whitespace()
        .collect()
}

fn assert_readme_lists_in_order(readme: &str, commands: impl IntoIterator<Item = impl AsRef<str>>) {
    let mut search_from = 0;
    for command in commands {
        let command = command.as_ref();
        let offset = readme[search_from..]
            .find(command)
            .unwrap_or_else(|| panic!("README should list command `{command}`"));
        search_from += offset + command.len();
    }
}

#[test]
fn readme_lists_just_ci_commands_in_order() {
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");
    let readme = std::fs::read_to_string(workspace_file("README.md")).expect("read README");
    let commands = just_check_recipes(&justfile)
        .into_iter()
        .flat_map(|recipe| just_recipe_commands(&justfile, recipe))
        .collect::<Vec<_>>();

    assert!(
        !commands.is_empty(),
        "justfile check recipe should expand to commands"
    );
    assert_readme_lists_in_order(&readme, commands);
}

#[test]
fn readme_lists_just_smoke_commands_in_order() {
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");
    let readme = std::fs::read_to_string(workspace_file("README.md")).expect("read README");
    let smoke_commands = just_recipe_commands(&justfile, "smoke");

    assert!(
        !smoke_commands.is_empty(),
        "justfile should define smoke commands"
    );
    assert_readme_lists_in_order(&readme, smoke_commands);
}
