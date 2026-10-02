use ecotokens::config::env_file::{load_from, parse};

#[test]
fn parses_comments_quotes_and_export() {
    let got = parse("# c\n\nA=1\nexport B = two \nC=\"x y\"\nD='z'\nbad line\n1X=no\nE=\n");
    assert_eq!(
        got,
        vec![
            ("A".into(), "1".into()),
            ("B".into(), "two".into()),
            ("C".into(), "x y".into()),
            ("D".into(), "z".into()),
            ("E".into(), "".into()),
        ]
    );
}

#[test]
fn real_environment_wins_and_file_fills_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".env");
    std::fs::write(&path, "ECOT_TEST_SET=from_file\nECOT_TEST_NEW=from_file\n").unwrap();
    std::env::set_var("ECOT_TEST_SET", "from_env");
    let applied = load_from(&path);
    assert_eq!(std::env::var("ECOT_TEST_SET").unwrap(), "from_env");
    assert_eq!(std::env::var("ECOT_TEST_NEW").unwrap(), "from_file");
    assert_eq!(applied, vec!["ECOT_TEST_NEW".to_string()]);
    assert!(load_from(&dir.path().join("missing")).is_empty());
}
