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

fn readme_raw_commands(readme: &str) -> Vec<&str> {
    let raw_heading = "Equivalent raw commands:";
    let after_heading = readme
        .split_once(raw_heading)
        .map(|(_, suffix)| suffix)
        .expect("README should include equivalent raw commands heading");
    let after_opening_fence = after_heading
        .split_once("```sh")
        .map(|(_, suffix)| suffix)
        .expect("README raw commands should use a shell code block");
    let block = after_opening_fence
        .split_once("```")
        .map(|(block, _)| block)
        .expect("README raw commands shell block should be closed");
    block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
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
    let raw_commands = readme_raw_commands(&readme);
    assert_eq!(
        &raw_commands[..commands.len()],
        commands.as_slice(),
        "README raw command block should start with just ci commands"
    );
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
    let check_command_count = just_check_recipes(&justfile)
        .into_iter()
        .flat_map(|recipe| just_recipe_commands(&justfile, recipe))
        .count();
    let raw_commands = readme_raw_commands(&readme);
    assert_eq!(
        &raw_commands[check_command_count..],
        smoke_commands.as_slice(),
        "README raw command block should end with just smoke commands"
    );
}
