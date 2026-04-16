use ecotokens::config::settings::Settings;
use ecotokens::filter::ai_summary;

#[test]
fn fallback_works_when_feature_disabled() {
    let settings = Settings::default();
    let output = "Line 1\nLine 2\nLine 3\n";
    let result = ai_summary::ai_summary_or_fallback(output, &settings);

    // Small output: passthrough without compression marker
    assert!(result.contains("Line 1"));
}

#[test]
fn fallback_handles_large_output() {
    let settings = Settings::default();
    let mut large_output = String::new();
    for i in 0..100 {
        large_output.push_str(&format!("Line {}\n", i));
    }

    let result = ai_summary::ai_summary_or_fallback(&large_output, &settings);

    // Should compress via fallback
    assert!(result.len() < large_output.len());
    assert!(result.contains("ecotokens"));
}

#[cfg(feature = "ai-summary")]
#[test]
fn respects_disabled_flag_in_settings() {
    let mut settings = Settings::default();
    settings.ai_summary_enabled = false;

    let output = "a".repeat(10000); // Large enough to trigger AI
    let result = ai_summary::ai_summary_or_fallback(&output, &settings);

    // Should fallback because disabled in settings
    assert!(result.contains("[ecotokens]"));
    assert!(!result.contains("AI summary"));
}

#[cfg(feature = "ai-summary")]
#[test]
fn skips_summary_for_large_json_object() {
    let mut settings = Settings::default();
    settings.ai_summary_enabled = true;
    settings.ai_summary_min_tokens = 10;

    let mut json = String::from("{\"items\":[");
    for i in 0..500 {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!("{{\"id\":{},\"name\":\"item-{}\"}}", i, i));
    }
    json.push_str("]}");

    let result = ai_summary::ai_summary_or_fallback(&json, &settings);

    assert!(
        !result.contains("AI summary ("),
        "JSON ne doit pas être résumé"
    );
    assert!(
        result.contains("\"id\":0"),
        "Doit conserver le début du JSON"
    );
}

#[cfg(feature = "ai-summary")]
#[test]
fn skips_summary_for_json_array() {
    let mut settings = Settings::default();
    settings.ai_summary_enabled = true;
    settings.ai_summary_min_tokens = 10;

    let json = format!(
        "[{}]",
        (0..500)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    let result = ai_summary::ai_summary_or_fallback(&json, &settings);

    assert!(!result.contains("AI summary ("));
}

#[cfg(feature = "ai-summary")]
#[test]
fn does_not_skip_invalid_json_lookalike() {
    let mut settings = Settings::default();
    settings.ai_summary_enabled = true;
    settings.ai_summary_min_tokens = 10;

    let fake = "{ not actually json at all, just looks like it ".repeat(200);
    let result = ai_summary::ai_summary_or_fallback(&fake, &settings);

    assert!(!result.is_empty());
}
