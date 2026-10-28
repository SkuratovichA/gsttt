//! The key a person's trust in an external connector is recorded under.

use std::fs;
use std::io;

use camino::{Utf8Path, Utf8PathBuf};
use gsttt_core::engine::keys::canonical_json;
use gsttt_core::hex;
use gsttt_core::setup::ExternalSetup;
use sha2::{Digest, Sha256};

/// Why the key of an external cannot be made.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// A file that the command names, and so that trust must cover, cannot be read.
    #[error(
        "cannot read `{path}`, which the command of the external connector `{name}` runs: {source}"
    )]
    Read {
        /// The name of the external.
        name: String,
        /// The file.
        path: Utf8PathBuf,
        /// What the system said.
        source: io::Error,
    },

    /// The setup of the external does not turn into JSON, which a setup read from a config always does.
    #[error("cannot describe the external connector `{name}` as JSON: {source}")]
    Setup {
        /// The name of the external.
        name: String,
        /// What serde said.
        source: serde_json::Error,
    },
}

/// The key of `external` in the repository at `root`, a canonical path: the SHA-256, as lowercase hex, of
///
/// 1. the canonical JSON of the setup: name, command, files, version, environment, provides and options;
/// 2. for each element of the command, in order, that names a regular file inside `root` once read against it: the
///    path of that file from `root`, then its bytes.
///
/// Every part is written as its length in bytes, as eight bytes in big-endian order, and then the part itself, so
/// that no two lists of parts make the same input. Elements that name nothing, a file outside the root or a
/// directory add nothing: they are the program name looked up in `PATH`, flags and their values.
///
/// # Errors
///
/// When the setup does not turn into JSON, or a file that the command names exists and cannot be read. A key that
/// left that file out would stay the same when the file changed.
pub fn key(root: &Utf8Path, external: &ExternalSetup) -> Result<String, KeyError> {
    let name = || external.name.clone();
    let setup = serde_json::to_value(external).map_err(|source| KeyError::Setup {
        name: name(),
        source,
    })?;
    let mut hasher = Sha256::new();
    feed(&mut hasher, canonical_json(&setup).as_bytes());
    for argument in &external.cmd {
        let Some((path, resolved)) = file_in(root, argument) else {
            continue;
        };
        let bytes = fs::read(&resolved).map_err(|source| KeyError::Read {
            name: name(),
            path: resolved,
            source,
        })?;
        feed(&mut hasher, path.as_str().as_bytes());
        feed(&mut hasher, &bytes);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// The regular file that `argument` names when read against `root`, as its path from `root` and its resolved path.
/// `None` when it names nothing, a directory or a file outside the root. A link that leads out of the root is
/// followed, and so is left out.
fn file_in(root: &Utf8Path, argument: &str) -> Option<(Utf8PathBuf, Utf8PathBuf)> {
    let resolved = root.join(argument).canonicalize_utf8().ok()?;
    let inside = resolved.strip_prefix(root).ok()?.to_owned();
    resolved.is_file().then_some((inside, resolved))
}

/// Writes `part` to `hasher` after its length.
fn feed(hasher: &mut Sha256, part: &[u8]) {
    let length = u64::try_from(part.len()).unwrap_or(u64::MAX);
    hasher.update(length.to_be_bytes());
    hasher.update(part);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    /// A folder that is its own canonical path, which is what a root is.
    fn root() -> (TempDir, Utf8PathBuf) {
        let dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
        (dir, root)
    }

    /// An external with every field set, and `changes` laid over it.
    fn external(changes: &serde_json::Value) -> ExternalSetup {
        let mut setup = json!({
            "name": "kotlin",
            "cmd": ["gsttt-kotlin", "--stdio"],
            "files": ["**/*.kt"],
            "version": "1",
            "env": ["JAVA_HOME"],
            "provides": [{"kind": "http", "role": "calls", "via": ["okhttp"], "blind_spots": []}],
            "options": {"jdk": 21},
        });
        if let (Some(base), Some(changes)) = (setup.as_object_mut(), changes.as_object()) {
            base.extend(changes.clone());
        }
        serde_json::from_value(setup).unwrap()
    }

    #[test]
    fn the_key_is_sixty_four_lowercase_hex_digits_and_repeats() {
        let (_dir, root) = root();
        let made = key(&root, &external(&json!({}))).unwrap();
        assert_eq!(made.len(), 64);
        assert!(
            made.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        assert_eq!(made, key(&root, &external(&json!({}))).unwrap());
    }

    #[test]
    fn every_field_of_the_setup_is_part_of_the_key() {
        let (_dir, root) = root();
        let base = key(&root, &external(&json!({}))).unwrap();
        for change in [
            json!({"name": "java"}),
            json!({"cmd": ["gsttt-kotlin"]}),
            json!({"files": ["**/*.kts"]}),
            json!({"version": "2"}),
            json!({"env": []}),
            json!({"provides": []}),
            json!({"options": {"jdk": 17}}),
        ] {
            assert_ne!(key(&root, &external(&change)).unwrap(), base, "{change}");
        }
    }

    #[test]
    fn the_order_the_options_are_written_in_does_not_matter() {
        let (_dir, root) = root();
        let first = external(&json!({"options": {"a": 1, "b": {"c": 2, "d": 3}}}));
        let second = external(&json!({"options": {"b": {"d": 3, "c": 2}, "a": 1}}));
        assert_eq!(key(&root, &first).unwrap(), key(&root, &second).unwrap());
    }

    #[test]
    fn a_file_the_command_names_is_part_of_the_key() {
        let (_dir, root) = root();
        fs::create_dir(root.join("tools")).unwrap();
        let script = root.join("tools/connector.py");
        fs::write(&script, "print(1)\n").unwrap();
        let python = external(&json!({"cmd": ["python3", "tools/connector.py"]}));

        let before = key(&root, &python).unwrap();
        assert_eq!(key(&root, &python).unwrap(), before);
        fs::write(&script, "print(2)\n").unwrap();

        assert_ne!(key(&root, &python).unwrap(), before);
    }

    #[test]
    fn the_key_hashes_the_framed_parts_in_order() {
        let (_dir, root) = root();
        fs::write(root.join("run.sh"), "echo hi\n").unwrap();
        let setup = external(&json!({"cmd": ["sh", "run.sh"]}));
        let canonical = canonical_json(&serde_json::to_value(&setup).unwrap());

        let mut expected = Sha256::new();
        for part in [canonical.as_bytes(), b"run.sh", b"echo hi\n"] {
            expected.update(u64::try_from(part.len()).unwrap().to_be_bytes());
            expected.update(part);
        }

        assert_eq!(
            key(&root, &setup).unwrap(),
            hex::encode(expected.finalize())
        );
    }

    #[test]
    fn what_the_command_names_outside_the_root_or_not_at_all_adds_nothing() {
        let (_dir, root) = root();
        let (_elsewhere, elsewhere) = self::root();
        let program = elsewhere.join("program");
        fs::write(&program, "one").unwrap();
        fs::create_dir(root.join("folder")).unwrap();
        let setup = external(&json!({"cmd": [program.as_str(), "folder", "missing", "--flag"]}));

        let before = key(&root, &setup).unwrap();
        fs::write(&program, "two").unwrap();
        fs::write(root.join("folder/inside"), "three").unwrap();

        assert_eq!(key(&root, &setup).unwrap(), before);
    }

    #[test]
    fn the_same_file_is_read_whatever_the_argument_points_through() {
        let (_dir, root) = root();
        fs::create_dir(root.join("tools")).unwrap();
        let script = root.join("tools/run.sh");
        fs::write(&script, "one").unwrap();
        let through_dots = external(&json!({"cmd": ["sh", "tools/../tools/run.sh"]}));
        let absolute = external(&json!({"cmd": ["sh", script.as_str()]}));

        let before = (
            key(&root, &through_dots).unwrap(),
            key(&root, &absolute).unwrap(),
        );
        fs::write(&script, "two").unwrap();

        assert_ne!(key(&root, &through_dots).unwrap(), before.0);
        assert_ne!(key(&root, &absolute).unwrap(), before.1);
    }

    /// The known gap that the design lists among its risks: a link is followed, and a target outside the root is not
    /// covered.
    #[cfg(unix)]
    #[test]
    fn a_link_that_leads_out_of_the_root_is_followed_and_left_out() {
        let (_dir, root) = root();
        let (_elsewhere, elsewhere) = self::root();
        let target = elsewhere.join("program");
        fs::write(&target, "one").unwrap();
        std::os::unix::fs::symlink(&target, root.join("program")).unwrap();
        let setup = external(&json!({"cmd": ["program"]}));

        let before = key(&root, &setup).unwrap();
        fs::write(&target, "two").unwrap();

        assert_eq!(key(&root, &setup).unwrap(), before);
    }
}
