use super::identity::AgentIdentity;

#[derive(Debug, Clone)]
pub struct AgentIntent {
    pub goal: String,
    pub constraints: Vec<String>,
    pub requested_tools: Vec<String>,
    pub output_contract: String,
    pub context_hints: Vec<String>,
}

pub fn build_intent(identity: &AgentIdentity, goal: &str) -> AgentIntent {
    let mut constraints = Vec::new();
    for item in &identity.must_never {
        constraints.push(format!("must_never:{item}"));
    }
    for item in &identity.must_always {
        constraints.push(format!("must_always:{item}"));
    }

    let requested_tools = vec![
        "search_memories".to_string(),
        "impact_query".to_string(),
        "create_diff".to_string(),
    ];

    let context_hints = vec![
        format!("risk_tolerance:{}", identity.risk_tolerance),
        format!("planning_depth:{}", identity.planning_depth),
        format!("communication_style:{}", identity.communication_style),
    ];

    AgentIntent {
        goal: goal.to_string(),
        constraints,
        requested_tools,
        output_contract: "diff_proposal".to_string(),
        context_hints,
    }
}
