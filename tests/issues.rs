use running_drafts_editor::document::{Document, VisibleTokenId};
use serde_json::json;

#[test]
fn editing_a_resolved_token_invalidates_resolution_and_undo_restores_it() {
    let id = |token_index| VisibleTokenId::Recognition {
        run_id: "run".into(),
        segment_id: "segment".into(),
        token_index,
    };
    let mut document: Document = serde_json::from_value(json!({
        "schema":"rde-document/v1-experimental",
        "id":"document:issues",
        "paragraphs":[{"id":"p","revision":1,"tokens":[
            {"id":id(0),"text":"bad","origin":{"kind":"recognition"}},
            {"id":id(1),"text":" token","origin":{"kind":"recognition"}}
        ],"chunk_boundaries":[{"chunk_id":"c","after_tokens":2}]}]
    }))
    .unwrap();

    document.resolve_issue(vec![id(0), id(1)]);
    document.replace_text(1, 1, 1, 1, "fixed".into()).unwrap();
    assert!(document.resolved_issues().is_empty());

    assert_eq!(document.undo(1), 1);
    assert_eq!(document.resolved_issues().len(), 1);
    assert_eq!(document.resolved_issues()[0].token_ids(), &[id(0), id(1)]);
}
