mod common;
#[test]
fn another_transcription_invalidates_issues_and_undo_restores_them() {
    let mut project = common::project(&["bad"]);
    let id = project.chunk_token(1, 1, 1).unwrap().id().clone();
    project.resolve_issue(vec![id.clone()]);
    project
        .install_transcription(
            1,
            1,
            common::proposal(&project, common::batch("later", &["fixed"])),
            project.settings().clone(),
        )
        .unwrap();
    assert!(project.resolved_issues().is_empty());
    project.undo(1);
    assert_eq!(project.resolved_issues()[0].token_identities(), &[id]);
}
