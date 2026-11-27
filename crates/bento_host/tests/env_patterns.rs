use bento_host::{EnvPattern, is_always_kept};

#[test]
fn a_name_matches_only_itself() {
    let pattern = EnvPattern::parse("PORT").unwrap();

    assert!(pattern.matches("PORT"));
    assert!(!pattern.matches("PORTS"));
    assert!(!pattern.matches("port"), "names are case-sensitive, as on macOS and Linux");
}

#[test]
fn a_prefix_followed_by_a_wildcard_matches_every_name_beginning_with_it() {
    let pattern = EnvPattern::parse("APP_*").unwrap();

    assert!(pattern.matches("APP_"));
    assert!(pattern.matches("APP_PORT"));
    assert!(!pattern.matches("APP"));
    assert!(!pattern.matches("MY_APP_PORT"));
}

#[test]
fn only_names_and_name_prefixes_are_patterns() {
    for invalid in ["", "*", "A*B", "**", "A=B", "=", "A=*"] {
        assert!(EnvPattern::parse(invalid).is_err(), "{invalid:?} is not a pattern");
    }
    for valid in ["A", "APP_*", "a.b-c", "NODE_ENV"] {
        assert_eq!(EnvPattern::parse(valid).unwrap().to_string(), valid);
    }
}

#[test]
fn nodes_always_kept_variables_include_the_locale_prefix() {
    for kept in ["PATH", "HOME", "NODE_OPTIONS", "TMPDIR", "LC_ALL", "LC_CTYPE", "TZ", "CI"] {
        assert!(is_always_kept(kept), "{kept} is always kept");
    }
    for removed in ["NODE_ENV", "USER", "SHELL", "PATHS", "LC", "DATABASE_URL"] {
        assert!(!is_always_kept(removed), "{removed} is not always kept");
    }
}
