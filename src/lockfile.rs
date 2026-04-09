use std::io;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const LOCKFILE_NAMES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Cargo.lock",
    "go.sum",
    "Gemfile.lock",
    "poetry.lock",
];

pub fn find_lockfiles(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    WalkDir::new(root)
        .max_depth(max_depth)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.file_name()
                    .to_str()
                    .map(|name| LOCKFILE_NAMES.contains(&name))
                    .unwrap_or(false)
        })
        .map(|e| e.into_path())
        .collect()
}

pub fn package_in_lockfile(lockfile: &Path, package: &str) -> Result<bool, io::Error> {
    let contents = std::fs::read_to_string(lockfile)?;
    Ok(contents.contains(package))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::fs;
    use tempfile::TempDir;

    proptest! {
        #[test]
        fn prop_finds_package_when_present(
            pkg in "[a-z][a-z0-9_-]{1,20}",
            filler in "[a-zA-Z0-9 \n\":{},._/-]{0,100}",
        ) {
            let dir = TempDir::new().unwrap();
            let lf = dir.path().join("package-lock.json");
            fs::write(&lf, format!("{}\n\"{}\": \"1.0.0\"\n", filler, pkg)).unwrap();

            prop_assert!(package_in_lockfile(&lf, &pkg).unwrap());
            prop_assert!(find_lockfiles(dir.path(), 2).contains(&lf));
        }

        #[test]
        fn prop_missing_package_returns_false(
            pkg in "[a-z][a-z0-9_-]{1,20}",
            other in "[A-Z][A-Z0-9_-]{1,20}",
        ) {
            let dir = TempDir::new().unwrap();
            let lf = dir.path().join("package-lock.json");
            fs::write(&lf, format!("\"{}\":\"1.0.0\"\n", other)).unwrap();

            prop_assert!(!package_in_lockfile(&lf, &pkg).unwrap());
        }
    }

    #[test]
    fn respects_max_depth() {
        let dir = TempDir::new().unwrap();

        let shallow = dir.path().join("package-lock.json");
        fs::write(&shallow, "").unwrap();

        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let mid = sub.join("yarn.lock");
        fs::write(&mid, "").unwrap();

        let deep = sub.join("deep");
        fs::create_dir(&deep).unwrap();
        let deep_lf = deep.join("Cargo.lock");
        fs::write(&deep_lf, "").unwrap();

        let d1 = find_lockfiles(dir.path(), 1);
        assert!(d1.contains(&shallow));
        assert!(!d1.contains(&mid));

        let d2 = find_lockfiles(dir.path(), 2);
        assert!(d2.contains(&mid));
        assert!(!d2.contains(&deep_lf));

        let d3 = find_lockfiles(dir.path(), 3);
        assert!(d3.contains(&deep_lf));
    }
}
