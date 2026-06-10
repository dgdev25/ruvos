use serde_json::Value;

const ORCH_CONTEXT_SEP: &str = "\n\nPrevious artifact to consume:\n";

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

/// Parse `[<template> orchestration as <archetype>] <task>` prefix.
/// Returns `Some((template, task))` or `None` if the format doesn't match.
pub(super) fn parse_orch_prompt<'a>(
    archetype: &str,
    prompt: &'a str,
) -> Option<(&'a str, &'a str)> {
    let inner = prompt.strip_prefix('[')?;
    let (header, rest) = inner.split_once("] ")?;
    let suffix = format!(" orchestration as {archetype}");
    let template = header.strip_suffix(suffix.as_str())?;
    let task = rest.lines().next().unwrap_or("").trim();
    if task.is_empty() {
        return None;
    }
    Some((template, task))
}

/// Build an orchestration-style artifact for any archetype.
pub(super) fn build_orch_artifact(
    archetype: &str,
    template: &str,
    task: &str,
    full_prompt: &str,
) -> String {
    let plan_consumed = match full_prompt.find(ORCH_CONTEXT_SEP) {
        Some(pos) => &full_prompt[pos + ORCH_CONTEXT_SEP.len()..],
        None => "(no prior plan)",
    };
    let section = match archetype {
        "coordinator" => "Sub-agents to dispatch",
        "planner" => "Ordered Delivery Steps",
        _ => "Implementation",
    };
    let steps = if archetype == "planner" {
        let rest_hint = if task.contains("POST") || task.contains("GET") || task.contains('/') {
            "   - Return appropriate HTTP status codes (201 Created, 400, 409, 422).\n"
        } else {
            ""
        };
        format!(
            "1. **coder** — implement the feature described in the task.{rest_hint}\
             2. **tester** — write tests covering success and error paths.\n\
             3. **reviewer** — verify response shape, auth enforcement, and error handling.\n"
        )
    } else {
        "1. Analyze the task and plan consumed.\n\
         2. Identify the modules to touch.\n\
         3. Implement the changes.\n\
         4. Run `cargo check` to verify compilation.\n"
            .to_string()
    };
    format!(
        "# {archetype} agent\n\n\
         ## Template\n{template}\n\n\
         ## Task\n{task}\n\n\
         ## Plan Consumed\n{plan_consumed}\n\n\
         ## {section}\n\
         {steps}"
    )
}

/// Archetype-specific plan derived from the prompt.
pub(super) fn build_artifact(
    archetype: &str,
    prompt: &str,
    output_schema: Option<&Value>,
) -> String {
    let mut out = build_artifact_body(archetype, prompt);
    if output_schema.is_some() {
        out.push_str("\n\n## Structured Output\n\n```json\n{}\n```\n");
    }
    out
}

fn build_artifact_body(archetype: &str, prompt: &str) -> String {
    if let Some((template, task)) = parse_orch_prompt(archetype, prompt) {
        return build_orch_artifact(archetype, template, task, prompt);
    }
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
    if archetype == "planner" && prompt.contains("POST /users") {
        return format!(
            "# planner agent\n\n## Task\n{prompt}\n\n## Feature Plan: POST /users\n\
             ### Requirements\n\
             1. Route: `POST /users` → 201 Created with `{{ id, name, email }}`.\n\
             2. Validate `email` format (422 Unprocessable Entity on invalid input).\n\
             3. Auth gate: require a valid bearer token (401 Unauthorized if absent/invalid).\n\
             4. Persist user with a UUID `id`.\n\
             5. Return 409 Conflict if `email` is already registered.\n\n\
             ### Delivery Pipeline\n\
             1. **coder** — implement `post_users` handler with axum + uuid.\n\
             2. **tester** — write tests covering 201, 401, 409, 422.\n\
             3. **reviewer** — verify response shape, auth enforcement, and duplicate detection.\n"
        );
    }
    if archetype == "tester" && prompt.contains("POST /users") {
        return format!(
            "# tester agent\n\n## Task\n{prompt}\n\n## Test cases covering POST /users\n\
             1. **201 Created** — valid `{{ name, email }}` body + valid auth token → status 201 + `{{ id, name, email }}`.\n\
             2. **422 Unprocessable Entity** — malformed email (e.g. `\"not-an-email\"`) → status 422.\n\
             3. **401 Unauthorized** — missing or invalid bearer token → status 401.\n\
             4. **409 Conflict** — POST same email twice → second call returns status 409.\n"
        );
    }
    if archetype == "reviewer" && prompt.contains("POST /users") {
        return format!(
            "# reviewer agent\n\n## Task\n{prompt}\n\n## Correctness, security, and style findings\n\
             1. 201 response includes `id`, `name`, `email` — UUID format for `id`.\n\
             2. 422 validation uses the `validator` crate or equivalent; rejects blank names/emails.\n\
             3. Auth gate is applied at the router level, not inside the handler.\n\
             4. 409 check occurs before insert to avoid race conditions on the unique-email constraint.\n\
             5. Error bodies follow a consistent `{{ error: string }}` shape across 401/409/422.\n"
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
    format!(
        "# {archetype} agent\n\n## Task\n{prompt}\n\n## {focus}\n\
         1. Analyze the task: \"{prompt}\"\n\
         2. {focus}.\n\
         3. Produce the deliverable and report back.\n"
    )
}

/// Extract a JSON value from the last ```json ... ``` block in an artifact.
pub(super) fn extract_structured_output(content: &str) -> Option<Value> {
    let marker = "```json\n";
    let pos = content.rfind(marker)?;
    let rest = &content[pos + marker.len()..];
    let end = rest.find("\n```")?;
    serde_json::from_str(&rest[..end]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── build_artifact ────────────────────────────────────────────────────────

    #[test]
    fn coder_post_users_special_case_contains_axum_handler() {
        let out = build_artifact("coder", "build a POST /users endpoint", None);
        assert!(out.contains("coder agent"));
        assert!(out.contains("POST /users"));
        assert!(out.contains("post_users"), "must define the handler fn");
        assert!(out.contains("StatusCode::CREATED"), "must return 201");
    }

    #[test]
    fn coder_safe_add_special_case_contains_checked_add() {
        let out = build_artifact("coder", "write a safe add function", None);
        assert!(out.contains("checked_add"));
        assert!(out.contains("Option<i32>"));
        assert!(out.contains("safe_add"));
    }

    #[test]
    fn coder_rust_module_special_case_contains_checked_add() {
        let out = build_artifact("coder", "scaffold a Rust module", None);
        assert!(
            out.contains("checked_add"),
            "'Rust module' path must use the specialised output"
        );
    }

    #[test]
    fn tester_safe_add_covers_overflow_edge_cases() {
        let out = build_artifact("tester", "test safe_add", None);
        assert!(out.contains("tester agent"));
        assert!(out.contains("i32::MAX"), "must cover positive overflow");
        assert!(out.contains("i32::MIN"), "must cover negative overflow");
        assert!(out.contains("Some(5)"), "must cover the happy path");
    }

    #[test]
    fn reviewer_safe_add_references_primitive_and_type() {
        let out = build_artifact("reviewer", "review safe_add implementation", None);
        assert!(out.contains("reviewer agent"));
        assert!(out.contains("checked_add"));
        assert!(out.contains("Option<i32>"));
    }

    #[test]
    fn all_twelve_archetypes_produce_artifact_containing_their_name() {
        for archetype in VALID_ARCHETYPES {
            let out = build_artifact(archetype, "some generic task", None);
            assert!(
                out.contains(&format!("{archetype} agent")),
                "artifact for '{archetype}' must include its name"
            );
        }
    }

    #[test]
    fn generic_prompt_appears_verbatim_in_artifact() {
        let out = build_artifact("planner", "decompose the sprint backlog", None);
        assert!(out.contains("decompose the sprint backlog"));
    }

    #[test]
    fn with_output_schema_appends_structured_output_section() {
        let schema = serde_json::json!({"type": "object"});
        let out = build_artifact("architect", "design the cache layer", Some(&schema));
        assert!(out.contains("## Structured Output"));
        assert!(out.contains("```json"));
    }

    #[test]
    fn without_output_schema_omits_structured_output_section() {
        let out = build_artifact("security", "threat model the API", None);
        assert!(!out.contains("Structured Output"));
    }

    #[test]
    fn unknown_archetype_falls_back_to_generic_focus() {
        let out = build_artifact("wizard", "cast a spell", None);
        assert!(out.contains("cast a spell"));
        assert!(
            out.contains("Work plan"),
            "unknown archetype must use generic focus"
        );
    }

    // ── extract_structured_output ─────────────────────────────────────────────

    #[test]
    fn finds_valid_json_object_block() {
        let content = "preamble\n```json\n{\"key\": \"value\"}\n```\ntrailing";
        let v = extract_structured_output(content).expect("must parse");
        assert_eq!(v["key"], "value");
    }

    #[test]
    fn returns_none_when_no_block_present() {
        assert!(extract_structured_output("no code block here").is_none());
    }

    #[test]
    fn returns_none_for_invalid_json_in_block() {
        assert!(extract_structured_output("```json\nnot { valid } json\n```").is_none());
    }

    #[test]
    fn picks_last_block_when_multiple_present() {
        let content = "```json\n{\"first\":1}\n```\nmore\n```json\n{\"second\":2}\n```";
        let v = extract_structured_output(content).expect("must find last block");
        assert_eq!(v["second"], 2);
        assert!(v.get("first").is_none(), "must ignore the first block");
    }

    #[test]
    fn handles_empty_json_object() {
        let v = extract_structured_output("prefix\n```json\n{}\n```\nsuffix").unwrap();
        assert!(v.is_object() && v.as_object().unwrap().is_empty());
    }

    #[test]
    fn handles_json_array_value() {
        let v = extract_structured_output("```json\n[1,2,3]\n```").unwrap();
        assert!(v.is_array());
        assert_eq!(v.as_array().unwrap().len(), 3);
    }

    #[test]
    fn handles_nested_json_object() {
        let content = "```json\n{\"outer\": {\"inner\": 42}}\n```";
        let v = extract_structured_output(content).unwrap();
        assert_eq!(v["outer"]["inner"], 42);
    }
}
