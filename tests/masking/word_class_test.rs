//! `\w` in a `regex!` pattern is Unicode by default, which makes bounded
//! repetitions such as `\w{82}` very expensive to compile (about 94 ms in total
//! for the four patterns below, paid once by every process that masks). These
//! patterns are therefore written ASCII-only with `(?-u)`. Real credentials of
//! these services are ASCII, so nothing real stops being masked.

use ecotokens::masking::mask;

const PATTERNS_SOURCE: &str = include_str!("../../src/masking/patterns.rs");

/// `n` characters cycling through ASCII letters, digits, `_` and `-`.
fn token_chars(n: usize) -> String {
    "aZ09_-bY18cX27dW36eV45".chars().cycle().take(n).collect()
}

/// Same, without `-`, for patterns whose class is a plain `\w`.
fn word_chars(n: usize) -> String {
    "aZ09_bY18cX27dW36eV45".chars().cycle().take(n).collect()
}

#[test]
fn every_pattern_using_a_word_class_is_ascii_only() {
    let offenders: Vec<&str> = PATTERNS_SOURCE
        .lines()
        .filter(|l| l.contains("regex!(") && l.contains("\\w") && !l.contains("(?-u)"))
        .collect();
    assert!(
        offenders.is_empty(),
        "these patterns use a Unicode `\\w`; add `(?-u)` (see this file's header):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_scan_above_sees_the_patterns_it_guards() {
    // Guards against the scan passing for the wrong reason (moved file, renamed macro).
    let with_word_class = PATTERNS_SOURCE
        .lines()
        .filter(|l| l.contains("regex!(") && l.contains("\\w"))
        .count();
    assert!(with_word_class >= 4, "found {with_word_class}");
}

#[test]
fn real_ascii_credentials_are_still_masked() {
    let cases = [
        (format!("key AIza{} end", token_chars(35)), "[GCP_KEY]"),
        (
            format!("tok github_pat_{} end", word_chars(82)),
            "[GITHUB_TOKEN]",
        ),
        (
            format!("tok glpat-{} end", token_chars(20)),
            "[GITLAB_TOKEN]",
        ),
        (
            format!("pypi-AgEIcHlwaS5vcmc{} end", token_chars(60)),
            "[PYPI_TOKEN]",
        ),
    ];
    for (input, tag) in cases {
        let (out, redacted) = mask(&input);
        assert!(redacted, "{input}");
        assert!(out.contains(tag), "{tag} missing in {out}");
        assert!(out.ends_with(" end"), "the surrounding text is kept: {out}");
    }
}

#[test]
fn a_credential_followed_by_non_ascii_text_is_masked_and_the_text_kept() {
    let cases = [
        (format!("AIza{}é fin", token_chars(35)), "[GCP_KEY]é fin"),
        (
            format!("glpat-{}中文", token_chars(20)),
            "[GITLAB_TOKEN]中文",
        ),
    ];
    for (input, expected) in cases {
        let (out, redacted) = mask(&input);
        assert!(redacted, "{input}");
        assert_eq!(out, expected);
    }
}

#[test]
fn look_alikes_made_of_non_ascii_letters_are_not_credentials() {
    // Not a decision to leave secrets unmasked: no real token contains these
    // characters. What changes with `(?-u)` is only that such text is left alone.
    let cases = [
        format!("AIza{}", "中".repeat(35)),
        format!("github_pat_{}", "é".repeat(82)),
        format!("glpat-{}", "ß".repeat(20)),
        format!("pypi-AgEIcHlwaS5vcmc{}", "ñ".repeat(60)),
    ];
    for input in cases {
        let (out, redacted) = mask(&input);
        assert!(!redacted, "{input} was masked as {out}");
        assert_eq!(out, input);
    }
}
