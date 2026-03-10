use ecotokens::filter::aws::filter_aws;

#[test]
fn aws_json_output_is_minified() {
    let input = r#"{
  "Buckets": [
    {
      "Name": "my-bucket",
      "CreationDate": "2024-01-01T00:00:00Z"
    }
  ],
  "Owner": {
    "DisplayName": "user",
    "ID": "abc123"
  }
}"#;
    let out = filter_aws(input);
    // Output should be valid JSON but more compact
    assert!(out.contains("my-bucket"), "bucket name should be present");
    assert!(!out.contains("\n  "), "should not have indented whitespace");
}

#[test]
fn aws_non_json_uses_generic() {
    let mut input = String::new();
    for i in 0..110 {
        input.push_str(&format!("aws: line {}\n", i));
    }
    let out = filter_aws(&input);
    assert!(!out.is_empty(), "should return something");
    assert!(
        out.len() < input.len(),
        "non-JSON output should be truncated"
    );
}

#[test]
fn aws_short_json_passes_through() {
    let input = r#"{"Status": "ok"}"#;
    let out = filter_aws(input);
    assert!(out.contains("ok"), "short JSON should be kept");
}

#[test]
fn aws_truncation_does_not_split_utf8_codepoint() {
    // MAX_JSON_BYTES = 51200. compact = `{"k":"<filler>éé"}`.
    // With filler of 51193 'a' chars the compact length is 6+51193+4+2 = 51205 bytes.
    // Byte 51200 falls inside the first "é" (a 2-byte codepoint starting at 51199) —
    // a naive &compact[..51200] would panic; floor_char_boundary must prevent that.
    let filler = "a".repeat(51193);
    let input = format!("{{\"k\":\"{filler}éé\"}}");
    let out = filter_aws(&input);
    assert!(
        out.contains("…[truncated]"),
        "long JSON should be truncated"
    );
}
