//! Version comparison and resolution across detection strategies.
//!
//! Strategies, in precedence order: the launcher's own metadata, a version the user entered, then
//! the product version embedded in the game's executable. Confidence is the strategy that supplied
//! the shown value. Values are never ordered across incomparable schemes, and disagreement between
//! comparable values is reported, not silently resolved.

use std::cmp::Ordering;

/// Compare two version strings. Numeric dotted versions (`1.32`, `v1.32.0.0`) compare part by part,
/// with missing parts as 0. Anything else only compares as equal (case-insensitive) or not at all.
pub fn compare(a: &str, b: &str) -> Option<Ordering> {
    match (numeric_parts(a), numeric_parts(b)) {
        (Some(x), Some(y)) => {
            let len = x.len().max(y.len());
            let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
            Some((0..len).map(|i| at(&x, i).cmp(&at(&y, i))).find(|o| o.is_ne()).unwrap_or(Ordering::Equal))
        }
        _ => a.trim().eq_ignore_ascii_case(b.trim()).then_some(Ordering::Equal),
    }
}

fn numeric_parts(v: &str) -> Option<Vec<u64>> {
    let v = v.trim();
    let v = v.strip_prefix(['v', 'V']).unwrap_or(v);
    v.split('.').map(|p| p.parse::<u64>().ok()).collect::<Option<Vec<_>>>().filter(|p| !p.is_empty())
}

#[derive(Debug, PartialEq)]
pub struct Resolved {
    pub version: Option<String>,
    pub source: Option<String>,
    /// `manifest`, `launcher`, `user`, `executable` or `unknown`.
    pub confidence: &'static str,
    /// Two comparable sources disagree.
    pub conflict: bool,
}

/// Pick the version to show and detect conflicts. `launcher_source` ending in `_buildid` marks a
/// build id: a different scheme from version numbers, so it is never compared with them.
pub fn resolve(
    launcher: Option<&str>,
    launcher_source: Option<&str>,
    user: Option<&str>,
    executable: Option<&str>,
) -> Resolved {
    let launcher_is_build_id = launcher_source.is_some_and(|s| s.ends_with("_buildid"));
    let comparable: Vec<&str> =
        [launcher.filter(|_| !launcher_is_build_id), user, executable].into_iter().flatten().collect();
    let conflict = comparable
        .iter()
        .enumerate()
        .any(|(i, a)| comparable[i + 1..].iter().any(|b| compare(a, b).is_some_and(Ordering::is_ne)));

    let (version, source, confidence) = if let Some(v) = launcher {
        // RepakHub's own installs record the version of the signed manifest they came from.
        (Some(v), launcher_source, if launcher_source == Some("manifest") { "manifest" } else { "launcher" })
    } else if let Some(v) = user {
        (Some(v), Some("user"), "user")
    } else if let Some(v) = executable {
        (Some(v), Some("executable"), "executable")
    } else {
        (None, None, "unknown")
    };
    Resolved { version: version.map(String::from), source: source.map(String::from), confidence, conflict }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ordering::*;

    #[test]
    fn compares_numeric_versions_and_refuses_to_guess_otherwise() {
        assert_eq!(compare("1.32", "1.32.0.0"), Some(Equal));
        assert_eq!(compare("v1.10", "1.9"), Some(Greater));
        assert_eq!(compare("2.0.1", "2.0.10"), Some(Less));
        assert_eq!(compare("++Release-28.10-CL-1", "++release-28.10-cl-1"), Some(Equal));
        assert_eq!(compare("1.2-beta", "1.2"), None, "mixed schemes are not ordered");
        assert_eq!(compare("", "1.0"), None);
    }

    #[test]
    fn resolves_by_precedence_with_confidence() {
        let r = resolve(Some("1.32"), Some("gog_version"), Some("1.31"), Some("1.30"));
        assert_eq!((r.version.as_deref(), r.source.as_deref(), r.confidence), (Some("1.32"), Some("gog_version"), "launcher"));
        assert!(r.conflict);

        let r = resolve(None, None, Some("2.0"), Some("2.0.0.0"));
        assert_eq!((r.version.as_deref(), r.confidence, r.conflict), (Some("2.0"), "user", false));

        let r = resolve(None, None, None, Some("1.5.0.0"));
        assert_eq!((r.source.as_deref(), r.confidence), (Some("executable"), "executable"));

        assert_eq!(resolve(None, None, None, None).confidence, "unknown");
        assert_eq!(resolve(Some("1.1"), Some("manifest"), None, None).confidence, "manifest");
    }

    #[test]
    fn build_ids_are_never_compared_with_version_numbers() {
        let r = resolve(Some("12345678"), Some("steam_buildid"), None, Some("1.2.0.0"));
        assert!(!r.conflict, "a build id and a product version are different schemes");
        assert_eq!(r.confidence, "launcher");
    }
}
