// Test harness only: never change production CLI/env or search the system PATH.
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub fn binary(name: &str, override_env: &str, cargo_path: &str) -> PathBuf {
    let executable = std::env::current_exe()
        .unwrap_or_else(|e| panic!("test harness failure: locate test executable: {e}"));
    let override_path = std::env::var_os(override_env);
    resolve(
        name,
        override_env,
        override_path.as_deref(),
        &executable,
        Path::new(cargo_path),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn checked(path: &Path) -> Result<PathBuf, String> {
    // Canonical absolute result ensures Command never resolves a basename via PATH.
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !canonical.is_file() {
        return Err(format!("{} is not a regular file", canonical.display()));
    }
    Ok(canonical)
}

fn resolve(
    name: &str,
    override_env: &str,
    override_path: Option<&OsStr>,
    test_exe: &Path,
    cargo_path: &Path,
) -> Result<PathBuf, String> {
    if let Some(path) = override_path {
        if path.is_empty() {
            return Err(format!("test harness failure: {override_env} is empty"));
        }
        return checked(Path::new(path))
            .map_err(|e| format!("test harness failure: explicit {override_env}: {e}"));
    }
    let sibling = test_exe
        .parent()
        .ok_or_else(|| "test harness failure: test executable has no parent".to_string())?
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    match checked(&sibling) {
        Ok(path) => Ok(path),
        Err(sibling_error) => {
            // On Windows a baked-in Unix path isn't absolute, so cannot become
            // a coincidental working-directory or PATH lookup.
            let fallback = if cargo_path.is_absolute() {
                checked(cargo_path)
            } else {
                Err("Cargo fallback is not an absolute target-platform path".into())
            };
            fallback.map_err(|e| format!(
                "test harness failure: missing {name}; set {override_env} or bundle it beside the test exe. sibling: {sibling_error}; Cargo fallback: {e}"
            ))
        }
    }
}

#[cfg(test)]
#[path = "remote_bundle_tests.rs"]
mod tests;
