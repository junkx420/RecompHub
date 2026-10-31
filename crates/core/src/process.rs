//! Detect running games before their files are replaced.

use std::collections::BTreeSet;
use std::path::Path;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Compare paths the way Windows does: case-insensitive, `\\?\`-free.
fn comparable(path: &Path) -> String {
    let path = crate::paths::normalize(path);
    let text = path.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// Names of running processes whose executable lies inside `dir`.
pub fn running_in(dir: &Path) -> Vec<String> {
    let mut system = match System::new() {
        Ok(system) => system,
        Err(err) => {
            // Without a process list, locked files still stop the swap
            // (rename fails and the journal rolls back).
            tracing::warn!(%err, "cannot list processes");
            return Vec::new();
        }
    };
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let mut prefix = comparable(dir);
    if !prefix.ends_with('\\') {
        prefix.push('\\');
    }
    let names: BTreeSet<String> = system
        .processes()
        .values()
        .filter(|p| {
            p.exe()
                .is_some_and(|exe| comparable(exe).starts_with(&prefix))
        })
        .map(|p| p.name().to_string_lossy().into_owned())
        .collect();
    names.into_iter().collect()
}

/// Is a process with this executable name running (case-insensitive)?
pub fn is_running_named(name: &str) -> bool {
    let Ok(mut system) = System::new() else {
        return false;
    };
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    system
        .processes()
        .values()
        .any(|p| p.name().to_string_lossy().eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_own_process_and_ignores_other_folders() {
        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().unwrap();
        assert!(!running_in(dir).is_empty());
        let empty = tempfile::tempdir().unwrap();
        assert!(running_in(empty.path()).is_empty());
    }

    #[test]
    fn sibling_folder_with_same_prefix_does_not_match() {
        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().unwrap();
        // `deps` vs `dep`: a plain string prefix check would match this.
        let shorter = dir.with_file_name(
            &dir.file_name().unwrap().to_string_lossy()[..dir.file_name().unwrap().len() - 1],
        );
        assert!(running_in(&shorter).is_empty());
    }
}
