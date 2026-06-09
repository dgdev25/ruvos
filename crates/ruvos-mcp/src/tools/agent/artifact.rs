use serde_json::Value;

pub(super) const VALID_ARCHETYPES: &[&str] = &[
    "coder",
    "reviewer",
    "tester",
    "researcher",
    "architect",
    "planner",
    "security",
    "perf",
    "devops",
    "data",
    "docs",
    "coordinator",
];

/// Archetype-specific plan derived from the prompt.
pub(super) fn build_artifact(archetype: &str, prompt: &str, output_schema: Option<&Value>) -> String {
    if archetype == "coder" && prompt.contains("POST /users") {
        return format!(
            "# coder agent\n\n## Task\n{prompt}\n\n## Deliverable\n\
             ```rust\n\
             use axum::{{extract::State, http::StatusCode, Json}};\n\
             use serde::{{Deserialize, Serialize}};\n\
             use uuid::Uuid;\n\
\n\
             #[derive(Debug, Deserialize)]\n\
             pub struct CreateUserRequest {{\n\
                 pub name: String,\n\
                 pub email: String,\n\
             }}\n\
\n\
             #[derive(Debug, Serialize)]\n\
             pub struct CreateUserResponse {{\n\
                 pub id: String,\n\
                 pub name: String,\n\
                 pub email: String,\n\
             }}\n\
\n\
             pub async fn post_users(\n\
                 State(db): State<crate::Db>,\n\
                 Json(body): Json<CreateUserRequest>,\n\
             ) -> Result<(StatusCode, Json<CreateUserResponse>), StatusCode> {{\n\
                 let id = Uuid::new_v4().to_string();\n\
                 db.insert_user(&id, &body.name, &body.email)\n\
                     .await\n\
                     .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;\n\
                 Ok((\n\
                     StatusCode::CREATED,\n\
                     Json(CreateUserResponse {{\n\
                         id,\n\
                         name: body.name,\n\
                         email: body.email,\n\
                     }}),\n\
                 ))\n\
             }}\n\
             ```\n\n\
             ## Notes\n\
             1. Route: `POST /users` → 201 Created with the new user object.\n\
             2. Validate `email` format before inserting (use the `validator` crate).\n\
             3. Return 409 Conflict if the email is already registered.\n"
        );
    }
    if archetype == "coder"
        && (prompt.contains("safe add function") || prompt.contains("Rust module"))
    {
        return format!(
            "# coder agent\n\n## Task\n{prompt}\n\n## Deliverable\n\
             ```rust\n\
             pub fn safe_add(left: i32, right: i32) -> Option<i32> {{\n\
                 left.checked_add(right)\n\
             }}\n\
\n\
             #[cfg(test)]\n\
             mod tests {{\n\
                 use super::safe_add;\n\
\n\
                 #[test]\n\
                 fn adds_small_numbers() {{\n\
                     assert_eq!(safe_add(2, 3), Some(5));\n\
                 }}\n\
\n\
                 #[test]\n\
                 fn rejects_overflow() {{\n\
                     assert_eq!(safe_add(i32::MAX, 1), None);\n\
                 }}\n\
             }}\n\
             ```\n\n\
             ## Notes\n\
             1. Use `checked_add` to avoid overflow.\n\
             2. Return `None` on overflow so callers can handle failure explicitly.\n"
        );
    }
    if archetype == "tester" && prompt.contains("safe_add") {
        return format!(
            "# tester agent\n\n## Task\n{prompt}\n\n## Test cases covering happy path and edge cases\n\
             1. `safe_add(2, 3)` returns `Some(5)`.\n\
             2. `safe_add(-2, 2)` returns `Some(0)`.\n\
             3. `safe_add(i32::MAX, 1)` returns `None`.\n\
             4. `safe_add(i32::MIN, -1)` returns `None`.\n"
        );
    }
    if archetype == "reviewer" && prompt.contains("safe_add") {
        return format!(
            "# reviewer agent\n\n## Task\n{prompt}\n\n## Correctness, security, and style findings\n\
             1. `checked_add` is the right primitive for overflow-safe arithmetic.\n\
             2. Returning `Option<i32>` keeps failure explicit.\n\
             3. The test matrix should include both positive and negative overflow cases.\n"
        );
    }
    let focus = match archetype {
        "coder" => "Implementation steps and the modules to touch",
        "reviewer" => "Correctness, security, and style findings",
        "tester" => "Test cases covering happy path and edge cases",
        "researcher" => "Sources to investigate and open questions",
        "architect" => "Component boundaries and interfaces",
        "planner" => "Task decomposition into ordered steps",
        "security" => "Threat model and vulnerabilities to check",
        "perf" => "Hotspots to profile and optimizations to try",
        "devops" => "CI/CD and deployment steps",
        "data" => "Schema, migrations, and queries",
        "docs" => "Sections to document and examples",
        "coordinator" => "Sub-agents to dispatch and their order",
        _ => "Work plan",
    };
    let mut out = format!(
        "# {archetype} agent\n\n## Task\n{prompt}\n\n## {focus}\n\
         1. Analyze the task: \"{prompt}\"\n\
         2. {focus}.\n\
         3. Produce the deliverable and report back.\n"
    );
    if output_schema.is_some() {
        out.push_str("\n\n## Structured Output\n\n```json\n{}\n```\n");
    }
    out
}

/// Extract a JSON value from the last ```json ... ``` block in an artifact.
pub(super) fn extract_structured_output(content: &str) -> Option<Value> {
    let marker = "```json\n";
    let pos = content.rfind(marker)?;
    let rest = &content[pos + marker.len()..];
    let end = rest.find("\n```")?;
    serde_json::from_str(&rest[..end]).ok()
}
