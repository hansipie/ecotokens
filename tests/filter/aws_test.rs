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
    assert!(out.len() < input.len(), "non-JSON output should be truncated");
}

#[test]
fn aws_short_json_passes_through() {
    let input = r#"{"Status": "ok"}"#;
    let out = filter_aws(input);
    assert!(out.contains("ok"), "short JSON should be kept");
}
