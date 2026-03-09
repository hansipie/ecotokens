use ecotokens::filter::network::filter_network;

#[test]
fn curl_short_passes_through() {
    let input = r#"{"status": "ok"}"#;
    let out = filter_network("curl https://example.com", input);
    assert!(out.contains("ok"), "short curl output should pass through");
}

#[test]
fn curl_json_is_minified() {
    let mut progress = String::new();
    for _ in 0..20 {
        progress.push_str("  % Total    % Received % Xferd  Average Speed   Time\n");
    }
    progress.push_str(r#"{"key": "value", "nested": {"a": 1}}"#);
    let out = filter_network("curl -s https://api.example.com", &progress);
    assert!(out.contains("key"), "JSON key should be present");
    assert!(!out.contains("% Total"), "progress lines should be removed");
}

#[test]
fn wget_keeps_only_meaningful_lines() {
    let mut input = String::new();
    for _ in 0..20 {
        input.push_str("Resolving example.com... 93.184.216.34\n");
        input.push_str("Connecting to example.com|93.184.216.34|:443... connected.\n");
    }
    input.push_str("'index.html' saved [1234/1234]\n");
    let out = filter_network("wget https://example.com", &input);
    assert!(out.contains("saved"), "saved line should be kept");
    assert!(!out.contains("Resolving"), "noisy lines should be removed");
}

#[test]
fn wget_error_is_kept() {
    let mut input = String::new();
    for _ in 0..20 {
        input.push_str("Connecting...\n");
    }
    input.push_str("ERROR 404: Not Found.\n");
    let out = filter_network("wget https://example.com/missing", &input);
    assert!(out.contains("ERROR"), "error line should be kept");
}
