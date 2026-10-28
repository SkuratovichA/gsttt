//! Distribution names: the PEP 503 normaliser, the reader of one requirement and the import names that differ from the name of the distribution that ships them.

use std::borrow::Cow;

/// PEP 503 normalisation: lower case, each run of `-`, `_` and `.` becomes one `-`.
pub(crate) fn normalize_distribution(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut separator = false;
    for ch in name.chars() {
        if matches!(ch, '-' | '_' | '.') {
            separator = true;
            continue;
        }
        if separator {
            normalized.push('-');
            separator = false;
        }
        normalized.extend(ch.to_lowercase());
    }
    if separator {
        normalized.push('-');
    }
    normalized
}

/// The normalised distribution a requirement line names: `PyYAML>=6; python_version < "3.12"` gives `pyyaml`, `requests[socks]==2.0` gives `requests` and `pkg @ https://example.com/pkg.zip` gives `pkg`. `None` for a line that does not start with a name, such as an option (`-r base.txt`), a path or a URL.
pub(crate) fn requirement_name(requirement: &str) -> Option<String> {
    let text = requirement.trim();
    let end = text
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')))
        .unwrap_or(text.len());
    let name = text.get(..end)?;
    if !name.starts_with(|ch: char| ch.is_ascii_alphanumeric()) {
        return None;
    }
    // After the name come extras, a version specifier, a marker or ` @ url`. Anything else, such as the `+` of `git+https://…` or the `:` of `file:///…`, means the line is a URL or a path and no name.
    let after = text.get(end..)?.trim_start();
    if !after.is_empty() && !after.starts_with(['[', '(', '<', '>', '=', '!', '~', ';', '@', ',']) {
        return None;
    }
    Some(normalize_distribution(name))
}

/// [`normalize_distribution`] for a name that is likely to be normal already: a name of lower-case letters and digits is its own normal form, and most import names are, so it is borrowed.
fn normalized(name: &str) -> Cow<'_, str> {
    if name
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(normalize_distribution(name))
    }
}

/// The distributions that may ship the import `module` (a dotted name), most specific first, each normalised, and each worked out only when the caller asks for it. Most distributions ship a package named like themselves; the ones that do not are listed in [`known_distribution`].
pub(crate) fn distributions_for_import(module: &str) -> impl Iterator<Item = Cow<'_, str>> {
    let mut segments = module.split('.');
    let top = segments.next().unwrap_or_default();
    let second = segments.next();
    let pair = second
        .and_then(|second| module.get(..top.len() + 1 + second.len()))
        .and_then(known_distribution);
    let known = known_distribution(top);
    // A namespace distribution such as `ruamel.yaml` or `zope.interface` is named after its first two segments.
    let namespace = second.filter(|_| known.is_none());
    pair.into_iter()
        .map(Cow::Borrowed)
        .chain(
            namespace
                .into_iter()
                .map(move |second| Cow::Owned(normalize_distribution(&format!("{top}-{second}")))),
        )
        .chain(std::iter::once_with(move || {
            known.map_or_else(|| normalized(top), Cow::Borrowed)
        }))
}

/// Distributions whose import name differs from their name on PyPI.
fn known_distribution(module: &str) -> Option<&'static str> {
    Some(match module {
        "Crypto" | "Cryptodome" => "pycryptodome",
        "MySQLdb" => "mysqlclient",
        "OpenSSL" => "pyopenssl",
        "PIL" => "pillow",
        "_yaml" | "yaml" => "pyyaml",
        "bs4" => "beautifulsoup4",
        "corsheaders" => "django-cors-headers",
        "cv2" => "opencv-python",
        "dateutil" => "python-dateutil",
        "discord" => "discord-py",
        "django_filters" => "django-filter",
        "docx" => "python-docx",
        "dotenv" => "python-dotenv",
        "fitz" => "pymupdf",
        "git" => "gitpython",
        "google.protobuf" => "protobuf",
        "jose" => "python-jose",
        "jwt" => "pyjwt",
        "ldap" => "python-ldap",
        "magic" => "python-magic",
        "mpl_toolkits" => "matplotlib",
        "multipart" => "python-multipart",
        "nacl" => "pynacl",
        "pkg_resources" => "setuptools",
        "pptx" => "python-pptx",
        "rest_framework" => "djangorestframework",
        "serial" => "pyserial",
        "skimage" => "scikit-image",
        "sklearn" => "scikit-learn",
        "slugify" => "python-slugify",
        "socks" => "pysocks",
        "telegram" => "python-telegram-bot",
        "usb" => "pyusb",
        "zmq" => "pyzmq",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn names_are_normalised_the_pep_503_way() {
        assert_eq!(normalize_distribution("PyYAML"), "pyyaml");
        assert_eq!(
            normalize_distribution("Foo_Bar.baz--qux"),
            "foo-bar-baz-qux"
        );
        assert_eq!(normalize_distribution("zope.interface"), "zope-interface");
    }

    #[test]
    fn requirements_give_their_name() {
        let cases = [
            ("PyYAML>=6; python_version < \"3.12\"", Some("pyyaml")),
            ("requests[socks]==2.0", Some("requests")),
            ("pkg @ https://example.com/pkg.zip", Some("pkg")),
            ("Flask", Some("flask")),
            ("-r base.txt", None),
            ("git+https://example.com/x.git", None),
            ("./local", None),
            ("", None),
        ];
        for (line, want) in cases {
            assert_eq!(requirement_name(line).as_deref(), want, "{line}");
        }
    }

    fn shipped_by(module: &str) -> Vec<String> {
        distributions_for_import(module)
            .map(Cow::into_owned)
            .collect()
    }

    #[test]
    fn import_names_map_to_their_distribution() {
        assert_eq!(shipped_by("yaml.constructor"), ["pyyaml"]);
        assert_eq!(shipped_by("ruamel.yaml"), ["ruamel-yaml", "ruamel"]);
        assert_eq!(
            shipped_by("google.protobuf.message"),
            ["protobuf", "google-protobuf", "google"]
        );
        assert_eq!(shipped_by("flask_cors"), ["flask-cors"]);
        assert_eq!(shipped_by("requests"), ["requests"]);
    }

    #[test]
    fn a_name_that_is_normal_already_is_borrowed() {
        assert!(matches!(normalized("requests"), Cow::Borrowed("requests")));
        assert!(matches!(normalized("py3"), Cow::Borrowed("py3")));
        assert!(matches!(normalized(""), Cow::Borrowed("")));
        assert!(matches!(normalized("flask_cors"), Cow::Owned(_)));
        assert!(matches!(normalized("Flask"), Cow::Owned(_)));
        assert!(
            distributions_for_import("requests.adapters")
                .last()
                .is_some_and(|last| matches!(last, Cow::Borrowed("requests")))
        );
    }

    proptest! {
        #[test]
        fn normalising_twice_changes_nothing(name in "[A-Za-z0-9._-]{0,24}") {
            let once = normalize_distribution(&name);
            prop_assert_eq!(normalize_distribution(&once), once);
        }

        #[test]
        fn a_normalised_name_has_no_runs_or_capitals(name in "[A-Za-z0-9._-]{0,24}") {
            let normalized = normalize_distribution(&name);
            prop_assert!(!normalized.contains("--"));
            prop_assert!(!normalized.contains(['_', '.']));
            prop_assert!(!normalized.chars().any(|ch| ch.is_ascii_uppercase()));
        }

        #[test]
        fn the_borrowing_normaliser_agrees_with_the_owning_one(name in "\\PC{0,24}") {
            prop_assert_eq!(normalized(&name), normalize_distribution(&name));
        }

        #[test]
        fn a_requirement_name_is_what_precedes_its_specifier(
            name in "[A-Za-z][A-Za-z0-9._-]{0,12}",
            specifier in "(>=|==|<|~=|!=)[0-9]{1,2}(\\.[0-9]{1,2}){0,2}",
        ) {
            let line = format!("{name}{specifier}");
            prop_assert_eq!(requirement_name(&line), Some(normalize_distribution(&name)));
        }

        #[test]
        fn a_requirement_line_never_panics(line in "\\PC{0,40}") {
            let _name = requirement_name(&line);
        }
    }
}
