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
