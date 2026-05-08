use serde_json::json;
use skycode_runtime::skycore::strip_provider_fields;

#[test]
fn strips_openai_fields_at_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let raw = json!({
        "task_id": "task-42",
        "status": "ok",
        "summary": "normalized",
        "artifacts": [
            {
                "kind": "diff",
                "id": "p1",
                "patch_unified": "--- a\n+++ b\n",
                "affected_files": ["src/lib.rs"],
                "choices": [{"junk": true}]
            }
        ],
        "requires_approval": true,
        "choices": [{"message": {"content": "provider"}}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 12},
        "model": "gpt-4o"
    });

    let response = strip_provider_fields(raw)?;

    assert_eq!(response.task_id, "task-42");
    assert!(response.requires_approval);
    assert_eq!(response.artifacts.len(), 1);
    assert_eq!(response.artifacts[0].id, "p1");

    Ok(())
}
