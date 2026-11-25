//! Apply a downloaded update (M3, part 3): all or nothing.
//!
//! Every file being replaced or removed is first moved into `.repakhub/backup`, then staged files are
//! moved into place and re-verified. Any failure moves everything back. On success the install record
//! (`.repakhub/install.json`) is rewritten, staging is removed, and the backup of the previous version
//! is kept until the next update.

use crate::downloads::sha256_file;
use crate::manifest::{FileEntry, Manifest};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Written into every folder RepakHub installed from a signed manifest.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecord {
    pub source: String,
    pub game_id: String,
    pub name: String,
    pub version: String,
    /// Files RepakHub put here, so a later version can remove the ones it no longer ships.
    pub files: Vec<String>,
}

pub fn record_path(install: &Path) -> PathBuf {
    install.join(".repakhub/install.json")
}

pub fn staging_dir(install: &Path) -> PathBuf {
    install.join(".repakhub/staging")
}

fn backup_dir(install: &Path) -> PathBuf {
    install.join(".repakhub/backup")
}

pub fn read_record(install: &Path) -> Option<InstallRecord> {
    serde_json::from_str(&crate::discovery::read_capped(&record_path(install))?).ok()
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("{0} did not verify after being moved into place")]
    Verify(String),
    #[error("this folder holds a different game ({0})")]
    WrongGame(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

enum Step {
    Added(PathBuf),
    Moved { target: PathBuf, backup: PathBuf },
}

/// Move the files in `staged` (already verified in staging) into `install` for manifest `m`.
pub fn apply(install: &Path, m: &Manifest, staged: &[&FileEntry]) -> Result<(), ApplyError> {
    let previous = read_record(install);
    if let Some(p) = &previous
        && (p.source != m.source || p.game_id != m.game_id)
    {
        return Err(ApplyError::WrongGame(format!("{}/{}", p.source, p.game_id)));
    }
    let backup = backup_dir(install);
    let _ = fs::remove_dir_all(&backup); // the previous version's backup is superseded now
    fs::create_dir_all(&backup)?;

    let mut steps = vec![];
    let result = apply_steps(install, m, staged, previous.as_ref(), &backup, &mut steps);
    if result.is_err() {
        for step in steps.iter().rev() {
            let _ = match step {
                Step::Added(target) => fs::remove_file(target),
                Step::Moved { target, backup } => fs::rename(backup, target),
            };
        }
        return result;
    }
    let _ = fs::remove_dir_all(staging_dir(install));
    Ok(())
}

fn apply_steps(
    install: &Path,
    m: &Manifest,
    staged: &[&FileEntry],
    previous: Option<&InstallRecord>,
    backup: &Path,
    steps: &mut Vec<Step>,
) -> Result<(), ApplyError> {
    let move_to_backup = |rel: &str, steps: &mut Vec<Step>| -> io::Result<()> {
        let (target, saved) = (install.join(rel), backup.join(rel));
        if target.exists() {
            fs::create_dir_all(saved.parent().unwrap_or(backup))?;
            fs::rename(&target, &saved)?;
            steps.push(Step::Moved { target, backup: saved });
        }
        Ok(())
    };

    for f in staged {
        let target = install.join(&f.path);
        let existed = target.exists();
        move_to_backup(&f.path, steps)?;
        fs::create_dir_all(target.parent().unwrap_or(install))?;
        fs::rename(staging_dir(install).join(&f.path), &target)?;
        if !existed {
            steps.push(Step::Added(target));
        }
    }
    // Files the previous version installed that this one no longer ships. Paths come from our own
    // record on disk, but are checked like manifest paths before anything is touched.
    if let Some(prev) = previous {
        for rel in &prev.files {
            let safe = !rel.contains('\\') && crate::discovery::is_plain_relative(rel) && !rel.starts_with(".repakhub");
            if safe && !m.files.iter().any(|f| f.path.eq_ignore_ascii_case(rel)) {
                move_to_backup(rel, steps)?;
            }
        }
    }
    for f in staged {
        let target = install.join(&f.path);
        if !target.metadata().is_ok_and(|md| md.len() == f.size) || sha256_file(&target)? != f.sha256 {
            return Err(ApplyError::Verify(f.path.clone()));
        }
    }
    let record = InstallRecord {
        source: m.source.clone(),
        game_id: m.game_id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        files: m.files.iter().map(|f| f.path.clone()).collect(),
    };
    let tmp = install.join(".repakhub/install.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(&record).map_err(io::Error::other)?)?;
    fs::rename(tmp, record_path(install))?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn sha(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    }

    pub fn entry(path: &str, content: &[u8]) -> FileEntry {
        FileEntry { path: path.into(), size: content.len() as u64, sha256: sha(content), url: String::new() }
    }

    pub fn manifest(version: &str, files: Vec<FileEntry>) -> Manifest {
        Manifest {
            schema: 1,
            source: "test-source".into(),
            game_id: "demo".into(),
            name: "Demo".into(),
            version: version.into(),
            changelog: String::new(),
            hosts: vec![],
            files,
            rights_holder: false,
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("repakhub-apply-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn stage(install: &Path, path: &str, content: &[u8]) {
        let p = staging_dir(install).join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn installs_then_updates_replacing_and_removing_files() {
        let install = tmp("ok");
        stage(&install, "data/a.pak", b"a1");
        stage(&install, "old.txt", b"old");
        let v1 = manifest("1.0", vec![entry("data/a.pak", b"a1"), entry("old.txt", b"old")]);
        apply(&install, &v1, &v1.files.iter().collect::<Vec<_>>()).unwrap();
        assert_eq!(read_record(&install).unwrap().version, "1.0");

        stage(&install, "data/a.pak", b"a2");
        let v2 = manifest("1.1", vec![entry("data/a.pak", b"a2")]);
        apply(&install, &v2, &v2.files.iter().collect::<Vec<_>>()).unwrap();
        assert_eq!(fs::read(install.join("data/a.pak")).unwrap(), b"a2");
        assert!(!install.join("old.txt").exists(), "file dropped by the new version is removed");
        assert_eq!(fs::read(backup_dir(&install).join("old.txt")).unwrap(), b"old", "and kept in the backup");
        assert_eq!(fs::read(backup_dir(&install).join("data/a.pak")).unwrap(), b"a1");
        assert!(!staging_dir(&install).exists());
        assert_eq!(read_record(&install).unwrap().files, vec!["data/a.pak"]);
    }

    #[test]
    fn rolls_back_everything_when_a_step_fails() {
        let install = tmp("rollback");
        stage(&install, "a.pak", b"a1");
        stage(&install, "b.pak", b"b1");
        let v1 = manifest("1.0", vec![entry("a.pak", b"a1"), entry("b.pak", b"b1")]);
        apply(&install, &v1, &v1.files.iter().collect::<Vec<_>>()).unwrap();

        // v1.1 replaces a.pak and adds c.pak, but c.pak's staged content does not match its hash.
        stage(&install, "a.pak", b"a2");
        stage(&install, "c.pak", b"tampered");
        let v2 = manifest("1.1", vec![entry("a.pak", b"a2"), entry("b.pak", b"b1"), entry("c.pak", b"c2")]);
        let staged = [&v2.files[0], &v2.files[2]];
        assert!(matches!(apply(&install, &v2, &staged), Err(ApplyError::Verify(_))));
        assert_eq!(fs::read(install.join("a.pak")).unwrap(), b"a1", "replaced file restored");
        assert!(!install.join("c.pak").exists(), "added file removed");
        assert_eq!(read_record(&install).unwrap().version, "1.0", "record untouched");
    }

    #[test]
    fn refuses_another_games_folder() {
        let install = tmp("wrong");
        stage(&install, "a.pak", b"a");
        let v1 = manifest("1.0", vec![entry("a.pak", b"a")]);
        apply(&install, &v1, &v1.files.iter().collect::<Vec<_>>()).unwrap();
        let mut other = manifest("2.0", vec![]);
        other.game_id = "other".into();
        assert!(matches!(apply(&install, &other, &[]), Err(ApplyError::WrongGame(_))));
    }
}
