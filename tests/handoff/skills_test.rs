use ecotokens::handoff::skills::{HANDOFF_LOAD_SKILL, HANDOFF_SKILL};
use ecotokens::handoff::MARKER;

#[test]
fn handoff_skill_is_user_only_and_pre_approves_only_its_own_commands() {
    assert!(
        HANDOFF_SKILL.starts_with("---\n"),
        "front-matter comes first"
    );
    assert!(HANDOFF_SKILL.contains("name: handoff\n"));
    assert!(HANDOFF_SKILL.contains("disable-model-invocation: true"));
    assert!(HANDOFF_SKILL.contains("allowed-tools: Bash(ecotokens handoff *)"));
    assert!(HANDOFF_SKILL.contains(MARKER));
}

#[test]
fn handoff_skill_runs_both_steps_with_the_session_id() {
    assert!(HANDOFF_SKILL.contains("ecotokens handoff write --session ${CLAUDE_SESSION_ID}"));
    assert!(HANDOFF_SKILL.contains("ecotokens handoff set --session ${CLAUDE_SESSION_ID} --stdin"));
    for field in ["objective", "problem", "next_steps", "abandoned"] {
        assert!(HANDOFF_SKILL.contains(field), "{field}");
    }
}

#[test]
fn handoff_skill_tells_the_user_to_clear() {
    assert!(HANDOFF_SKILL.contains("/clear"));
}

#[test]
fn handoff_skill_stays_short_because_an_invoked_skill_stays_in_context() {
    assert!(
        HANDOFF_SKILL.lines().count() < 40,
        "{}",
        HANDOFF_SKILL.lines().count()
    );
    assert!(HANDOFF_LOAD_SKILL.lines().count() < 20);
}

#[test]
fn handoff_load_skill_loads_the_given_id() {
    assert!(HANDOFF_LOAD_SKILL.contains("name: handoff-load\n"));
    assert!(HANDOFF_LOAD_SKILL.contains("disable-model-invocation: true"));
    assert!(HANDOFF_LOAD_SKILL.contains("allowed-tools: Bash(ecotokens handoff *)"));
    assert!(HANDOFF_LOAD_SKILL.contains("ecotokens handoff load $ARGUMENTS"));
    assert!(HANDOFF_LOAD_SKILL.contains(MARKER));
}
