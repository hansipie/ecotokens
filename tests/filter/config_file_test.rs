use ecotokens::filter::config_file::filter_config_file;

#[test]
fn small_toml_passes_through() {
    let input = "[package]\nname = \"foo\"\nversion = \"1.0\"\n";
    let out = filter_config_file(input, "toml");
    assert_eq!(out, input);
}

#[test]
fn large_toml_shows_top_level_tables() {
    let mut input = String::new();
    for i in 0..120 {
        input.push_str(&format!("[section{i}]\nkey = \"value\"\n\n"));
    }
    let out = filter_config_file(&input, "toml");
    assert!(out.len() < input.len(), "output should be shorter");
    assert!(out.contains("[ecotokens]"), "should have summary marker");
}

#[test]
fn large_json_shows_root_keys() {
    let mut obj = serde_json::Map::new();
    for i in 0..120 {
        obj.insert(format!("key{i}"), serde_json::Value::String("value".into()));
    }
    let input = serde_json::to_string_pretty(&serde_json::Value::Object(obj)).unwrap();
    let out = filter_config_file(&input, "json");
    assert!(out.len() < input.len());
    assert!(out.contains("[ecotokens]"));
}

#[test]
fn small_json_passes_through() {
    let input = r#"{"name": "foo", "version": "1.0"}"#;
    let out = filter_config_file(input, "json");
    assert_eq!(out, input);
}
