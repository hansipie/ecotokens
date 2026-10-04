use ecotokens::handoff::skills::{
    are_skills_installed, install_skills, remove_skills, skill_path, HANDOFF_LOAD_SKILL,
    HANDOFF_SKILL,
};
use tempfile::TempDir;

#[test]
fn install_writes_both_skills_with_the_marker_and_the_exact_text() {
    let dir = TempDir::new().unwrap();
    let (written, kept) = install_skills(dir.path()).unwrap();
    assert_eq!(written.len(), 2);
    assert!(kept.is_empty());
    assert_eq!(
        std::fs::read_to_string(skill_path(dir.path(), "handoff")).unwrap(),
        HANDOFF_SKILL
    );
    assert_eq!(
        std::fs::read_to_string(skill_path(dir.path(), "handoff-load")).unwrap(),
        HANDOFF_LOAD_SKILL
    );
    assert!(are_skills_installed(dir.path()));
}

#[test]
fn install_is_idempotent() {
    let dir = TempDir::new().unwrap();
    install_skills(dir.path()).unwrap();
    let before = std::fs::read_to_string(skill_path(dir.path(), "handoff")).unwrap();
    install_skills(dir.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(skill_path(dir.path(), "handoff")).unwrap(),
        before
    );
    assert!(are_skills_installed(dir.path()));
}

#[test]
fn a_skill_the_user_wrote_is_never_overwritten() {
    let dir = TempDir::new().unwrap();
    let mine = skill_path(dir.path(), "handoff");
    std::fs::create_dir_all(mine.parent().unwrap()).unwrap();
    std::fs::write(&mine, "my own handoff skill").unwrap();
    let (written, kept) = install_skills(dir.path()).unwrap();
    assert_eq!(kept, vec![mine.clone()]);
    assert_eq!(written, vec![skill_path(dir.path(), "handoff-load")]);
    assert_eq!(
        std::fs::read_to_string(&mine).unwrap(),
        "my own handoff skill"
    );
    assert!(
        !are_skills_installed(dir.path()),
        "one of the two is not ours"
    );
}

#[test]
fn remove_deletes_only_managed_files_and_their_empty_folders() {
    let dir = TempDir::new().unwrap();
    install_skills(dir.path()).unwrap();
    // The user's own skill takes the place of one of ours.
    let mine = skill_path(dir.path(), "handoff-load");
    std::fs::write(&mine, "my own skill").unwrap();
    let removed = remove_skills(dir.path()).unwrap();
    assert_eq!(removed, vec![skill_path(dir.path(), "handoff")]);
    assert!(
        !dir.path().join("handoff").exists(),
        "the empty folder went with it"
    );
    assert!(mine.exists(), "the user's skill stays");
}

#[test]
fn remove_keeps_a_folder_that_holds_other_files() {
    let dir = TempDir::new().unwrap();
    install_skills(dir.path()).unwrap();
    std::fs::write(dir.path().join("handoff").join("notes.txt"), "keep").unwrap();
    remove_skills(dir.path()).unwrap();
    assert!(!skill_path(dir.path(), "handoff").exists());
    assert!(dir.path().join("handoff").join("notes.txt").exists());
}

#[test]
fn remove_is_idempotent_and_tolerates_a_missing_directory() {
    let dir = TempDir::new().unwrap();
    install_skills(dir.path()).unwrap();
    assert_eq!(remove_skills(dir.path()).unwrap().len(), 2);
    assert!(remove_skills(dir.path()).unwrap().is_empty());
    assert!(remove_skills(&dir.path().join("never-created"))
        .unwrap()
        .is_empty());
    assert!(!are_skills_installed(dir.path()));
}
