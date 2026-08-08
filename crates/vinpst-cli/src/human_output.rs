use std::path::Path;

pub(crate) fn print_config_mutation(
    dry_run: bool,
    preview: &str,
    applied: &str,
    output_path: Option<&Path>,
    backup_path: Option<&Path>,
) {
    println!("{}", if dry_run { preview } else { applied });
    if dry_run {
        return;
    }
    if let Some(path) = output_path {
        println!("Updated config: {}", path.display());
    }
    if let Some(path) = backup_path {
        println!("Backup: {}", path.display());
    }
}
