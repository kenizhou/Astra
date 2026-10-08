// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: VERSION and the workspace package version agree.

pub fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let version_file = root.join("VERSION");
    let cargo_file = root.join("Cargo.toml");
    if !version_file.is_file() || !cargo_file.is_file() {
        return Err(vec!["VERSION and Cargo.toml must both exist".to_string()]);
    }
    let declared = super::read_lossy(&version_file).trim().to_string();
    let Some(workspace) = workspace_version(&super::read_lossy(&cargo_file)) else {
        return Err(vec!["Cargo.toml has no [workspace.package] version".to_string()]);
    };
    if declared == workspace {
        Ok(())
    } else {
        Err(vec![format!("VERSION says {declared} but Cargo.toml says {workspace}")])
    }
}

/// Reads `version = "x.y.z"` from the `[workspace.package]` section.
pub fn workspace_version(cargo_toml: &str) -> Option<String> {
    let mut in_section = false;
    for line in cargo_toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t == "[workspace.package]";
            continue;
        }
        if in_section && t.starts_with("version") {
            let v = t.split('"').nth(1)?;
            return Some(v.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;

    const TOML: &str = "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"0.1.0\"\n";

    #[test]
    fn passes_when_versions_agree() {
        let root = fixture(&[("VERSION", "0.1.0\n"), ("Cargo.toml", TOML)]);
        assert!(super::check(&root).is_ok());
    }

    #[test]
    fn fails_when_versions_disagree() {
        let root = fixture(&[("VERSION", "0.2.0\n"), ("Cargo.toml", TOML)]);
        assert!(super::check(&root).is_err());
    }

    #[test]
    fn parser_reads_only_the_workspace_section() {
        let toml = "[package]\nversion = \"9.9.9\"\n\n[workspace.package]\nversion = \"0.1.0\"\n";
        assert_eq!(super::workspace_version(toml).as_deref(), Some("0.1.0"));
    }
}
