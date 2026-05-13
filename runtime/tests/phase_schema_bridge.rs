use serde_json::json;
use skycode_core::skycore::{strip_provider_fields, SkyCoreResponse};
use skycode_orchestrator::orchestrator::{task_loop::select_diff_artifact, OrchestratorError};

fn response_with_artifact(kind: &str) -> SkyCoreResponse {
    strip_provider_fields(json!({
        "skycore_version": "0.1",
        "task_id": "task-schema-bridge",
        "status": "ok",
        "summary": "created file",
        "artifacts": [
            {
                "kind": kind,
                "id": "x",
                "content": "hello"
            }
        ],
        "tool_calls_requested": [],
        "requires_approval": true,
        "error": null
    }))
    .expect("SkyCore response should deserialize")
}

#[test]
fn schema_bridge_kind_file_treated_as_rewrite() {
    let response = response_with_artifact("file");

    let artifact = select_diff_artifact(&response).expect("file artifact should normalize");
    let body = artifact
        .new_content
        .as_deref()
        .or(artifact.patch_unified.as_deref());

    assert_eq!(artifact.kind, "rewrite");
    assert_eq!(body, Some("hello"));
    assert_eq!(artifact.affected_files, Some(vec!["x".to_string()]));
}

#[test]
fn schema_bridge_kind_create_treated_as_rewrite() {
    let response = response_with_artifact("create");

    let artifact = select_diff_artifact(&response).expect("create artifact should normalize");
    let body = artifact
        .new_content
        .as_deref()
        .or(artifact.patch_unified.as_deref());

    assert_eq!(artifact.kind, "rewrite");
    assert_eq!(body, Some("hello"));
    assert_eq!(artifact.affected_files, Some(vec!["x".to_string()]));
}

#[test]
fn schema_bridge_kind_unknown_rejected() {
    let response = response_with_artifact("banana");

    let err = select_diff_artifact(&response).expect_err("unknown artifact kind should fail");

    assert!(matches!(err, OrchestratorError::MissingDiffArtifact));
}
