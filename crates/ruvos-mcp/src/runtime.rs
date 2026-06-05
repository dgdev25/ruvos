//! Runtime spine primitives for the agentic OS roadmap.
//!
//! This module is intentionally small: it defines the shared execution concepts
//! that later phases can wire into tools, schedulers, and policy checks.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use ruvos_store::EventRecord;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::store::try_store;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AutonomyMode {
    Manual,
    Assist,
    Delegate,
    Autopilot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Running,
    Blocked,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskNode {
    pub id: String,
    pub label: String,
    pub depends_on: Vec<String>,
    pub state: TaskState,
}

impl TaskNode {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            depends_on: Vec::new(),
            state: TaskState::Pending,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGraph {
    nodes: HashMap<String, TaskNode>,
}

impl TaskGraph {
    pub fn add_task(&mut self, node: TaskNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_dependency(&mut self, task_id: &str, dependency_id: &str) {
        if let Some(node) = self.nodes.get_mut(task_id) {
            if !node.depends_on.iter().any(|dep| dep == dependency_id) {
                node.depends_on.push(dependency_id.to_string());
            }
        }
    }

    pub fn set_state(&mut self, task_id: &str, state: TaskState) {
        if let Some(node) = self.nodes.get_mut(task_id) {
            node.state = state;
        }
    }

    pub fn task(&self, task_id: &str) -> Option<&TaskNode> {
        self.nodes.get(task_id)
    }

    pub fn ready_tasks(&self) -> Vec<&TaskNode> {
        self.nodes
            .values()
            .filter(|node| {
                node.state == TaskState::Pending
                    && node.depends_on.iter().all(|dep| {
                        matches!(
                            self.nodes.get(dep).map(|node| &node.state),
                            Some(TaskState::Completed)
                        )
                    })
            })
            .collect()
    }

    pub fn blocked_tasks(&self) -> Vec<&TaskNode> {
        self.nodes
            .values()
            .filter(|node| {
                node.state == TaskState::Blocked
                    || node.depends_on.iter().any(|dep| {
                        matches!(
                            self.nodes.get(dep).map(|node| &node.state),
                            Some(TaskState::Failed | TaskState::Blocked)
                        )
                    })
            })
            .collect()
    }

    pub fn completed(&self) -> bool {
        self.nodes
            .values()
            .all(|node| node.state == TaskState::Completed)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskScheduler {
    graph: TaskGraph,
    queue: VecDeque<String>,
}

impl TaskScheduler {
    pub fn new(graph: TaskGraph) -> Self {
        let mut scheduler = Self {
            graph,
            queue: VecDeque::new(),
        };
        scheduler.refresh_queue();
        scheduler
    }

    pub fn graph(&self) -> &TaskGraph {
        &self.graph
    }

    pub fn graph_mut(&mut self) -> &mut TaskGraph {
        &mut self.graph
    }

    pub fn refresh_queue(&mut self) {
        for task in self.graph.ready_tasks() {
            if !self.queue.iter().any(|queued| queued == &task.id) {
                self.queue.push_back(task.id.clone());
            }
        }
    }

    pub fn next_ready(&mut self) -> Option<TaskNode> {
        self.refresh_queue();
        while let Some(task_id) = self.queue.pop_front() {
            if matches!(
                self.graph.task(&task_id).map(|task| &task.state),
                Some(TaskState::Pending)
            ) {
                self.graph.set_state(&task_id, TaskState::Running);
                return self.graph.task(&task_id).cloned();
            }
        }
        None
    }

    pub fn mark_completed(&mut self, task_id: &str) {
        self.graph.set_state(task_id, TaskState::Completed);
        self.queue.retain(|queued| queued != task_id);
        self.refresh_queue();
    }

    pub fn mark_failed(&mut self, task_id: &str) {
        self.graph.set_state(task_id, TaskState::Failed);
        self.queue.retain(|queued| queued != task_id);
    }

    pub fn mark_blocked(&mut self, task_id: &str) {
        self.graph.set_state(task_id, TaskState::Blocked);
        self.queue.retain(|queued| queued != task_id);
    }

    pub fn completed(&self) -> bool {
        self.graph.completed()
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, bytes)
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicyScope {
    Tool(String),
    File(String),
    Network(String),
    Destructive(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub mode: AutonomyMode,
    pub scope: PolicyScope,
    pub reason: String,
}

impl PolicyDecision {
    pub fn allow(mode: AutonomyMode, scope: PolicyScope, reason: impl Into<String>) -> Self {
        Self {
            allowed: true,
            mode,
            scope,
            reason: reason.into(),
        }
    }

    pub fn deny(mode: AutonomyMode, scope: PolicyScope, reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            mode,
            scope,
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimePolicy {
    pub mode: AutonomyMode,
    pub allow_all: bool,
    pub allow_destructive: bool,
    pub allowed_tools: HashSet<String>,
    pub allowed_files: HashSet<String>,
    pub allowed_networks: HashSet<String>,
}

impl RuntimePolicy {
    pub fn permissive(mode: AutonomyMode) -> Self {
        Self {
            mode,
            allow_all: true,
            allow_destructive: true,
            allowed_tools: HashSet::new(),
            allowed_files: HashSet::new(),
            allowed_networks: HashSet::new(),
        }
    }

    pub fn restrictive(mode: AutonomyMode) -> Self {
        Self {
            mode,
            allow_all: false,
            allow_destructive: false,
            allowed_tools: HashSet::new(),
            allowed_files: HashSet::new(),
            allowed_networks: HashSet::new(),
        }
    }

    pub fn authorize(&self, scope: PolicyScope) -> PolicyDecision {
        if self.allow_all {
            return PolicyDecision::allow(self.mode, scope, "policy allows all scopes");
        }

        let scope_clone = scope.clone();
        match scope_clone {
            PolicyScope::Tool(tool) => {
                if self.allowed_tools.contains(&tool) {
                    PolicyDecision::allow(self.mode, scope, "tool is allowed")
                } else {
                    PolicyDecision::deny(self.mode, scope, "tool is not allowlisted")
                }
            }
            PolicyScope::File(path) => {
                if self
                    .allowed_files
                    .iter()
                    .any(|allowed| path.starts_with(allowed))
                {
                    PolicyDecision::allow(self.mode, scope, "file scope is allowed")
                } else {
                    PolicyDecision::deny(self.mode, scope, "file scope is not allowlisted")
                }
            }
            PolicyScope::Network(host) => {
                if self.allowed_networks.contains(&host) {
                    PolicyDecision::allow(self.mode, scope, "network scope is allowed")
                } else {
                    PolicyDecision::deny(self.mode, scope, "network scope is not allowlisted")
                }
            }
            PolicyScope::Destructive(action) => {
                if self.allow_destructive {
                    PolicyDecision::allow(self.mode, scope, "destructive action allowed")
                } else {
                    PolicyDecision::deny(
                        self.mode,
                        scope,
                        format!("destructive action denied: {action}"),
                    )
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceBudget {
    pub max_elapsed_ms: Option<u64>,
    pub max_tool_calls: Option<u64>,
    pub max_tokens: Option<u64>,
    pub max_retries: Option<u64>,
}

impl ResourceBudget {
    pub fn permissive() -> Self {
        Self {
            max_elapsed_ms: None,
            max_tool_calls: None,
            max_tokens: None,
            max_retries: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ResourceUsage {
    pub elapsed_ms: u64,
    pub tool_calls: u64,
    pub tokens: u64,
    pub retries: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceTracker {
    pub budget: ResourceBudget,
    pub usage: ResourceUsage,
}

impl ResourceTracker {
    pub fn permissive() -> Self {
        Self {
            budget: ResourceBudget::permissive(),
            usage: ResourceUsage::default(),
        }
    }

    pub fn restrictive(max_tool_calls: u64) -> Self {
        Self {
            budget: ResourceBudget {
                max_elapsed_ms: None,
                max_tool_calls: Some(max_tool_calls),
                max_tokens: None,
                max_retries: None,
            },
            usage: ResourceUsage::default(),
        }
    }

    pub fn can_start_tool(&self) -> bool {
        match self.budget.max_tool_calls {
            Some(limit) => self.usage.tool_calls < limit,
            None => true,
        }
    }

    pub fn record_tool_call(&mut self, elapsed_ms: u64) {
        self.usage.tool_calls = self.usage.tool_calls.saturating_add(1);
        self.usage.elapsed_ms = self.usage.elapsed_ms.saturating_add(elapsed_ms);
    }

    pub fn record_tokens(&mut self, tokens: u64) {
        self.usage.tokens = self.usage.tokens.saturating_add(tokens);
    }

    pub fn record_retry(&mut self) {
        self.usage.retries = self.usage.retries.saturating_add(1);
    }

    pub fn is_exhausted(&self) -> bool {
        self.budget
            .max_tool_calls
            .is_some_and(|limit| self.usage.tool_calls >= limit)
            || self
                .budget
                .max_elapsed_ms
                .is_some_and(|limit| self.usage.elapsed_ms >= limit)
            || self
                .budget
                .max_tokens
                .is_some_and(|limit| self.usage.tokens >= limit)
            || self
                .budget
                .max_retries
                .is_some_and(|limit| self.usage.retries >= limit)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailureClass {
    Transient,
    Validation,
    Permission,
    Dependency,
    External,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RepairAction {
    Retry,
    Rework,
    FixInput,
    RequestPermission,
    Escalate,
    Abort,
}

pub fn classify_failure(message: &str) -> FailureClass {
    let lower = message.to_lowercase();
    if lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("temporarily")
        || lower.contains("transient")
        || lower.contains("retry")
    {
        FailureClass::Transient
    } else if lower.contains("invalid params")
        || lower.contains("validation")
        || lower.contains("malformed")
        || lower.contains("missing")
    {
        FailureClass::Validation
    } else if lower.contains("permission denied") || lower.contains("unauthorized") {
        FailureClass::Permission
    } else if lower.contains("not found")
        || lower.contains("missing dependency")
        || lower.contains("method not found")
    {
        FailureClass::Dependency
    } else if lower.contains("network")
        || lower.contains("io error")
        || lower.contains("connection")
        || lower.contains("external")
    {
        FailureClass::External
    } else {
        FailureClass::Unknown
    }
}

pub fn repair_action_for(class: FailureClass, retries_remaining: usize) -> RepairAction {
    match class {
        FailureClass::Transient if retries_remaining > 0 => RepairAction::Retry,
        FailureClass::Transient => RepairAction::Rework,
        FailureClass::Validation => RepairAction::FixInput,
        FailureClass::Permission => RepairAction::RequestPermission,
        FailureClass::Dependency => RepairAction::Rework,
        FailureClass::External if retries_remaining > 0 => RepairAction::Retry,
        FailureClass::External => RepairAction::Escalate,
        FailureClass::Unknown if retries_remaining > 0 => RepairAction::Retry,
        FailureClass::Unknown => RepairAction::Abort,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeEvent {
    pub kind: String,
    pub payload: Value,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
}

impl RuntimeEvent {
    pub fn new(kind: impl Into<String>, payload: Value) -> Self {
        Self {
            kind: kind.into(),
            payload,
            agent_id: None,
            task_id: None,
        }
    }
}

fn trace_envelope(kind: &str) -> Value {
    let parts: Vec<&str> = kind.split('.').collect();
    let category = parts.first().copied().unwrap_or(kind);
    let stage = parts.last().copied().unwrap_or(kind);
    let action = if parts.len() <= 2 {
        parts.get(1).copied().unwrap_or("").to_string()
    } else {
        parts[1..parts.len() - 1].join(".")
    };

    json!({
        "trace": {
            "kind": kind,
            "category": category,
            "action": action,
            "stage": stage,
            "path": parts,
        }
    })
}

fn normalize_payload(kind: &str, payload: Value) -> Value {
    let trace = trace_envelope(kind)["trace"].clone();
    match payload {
        Value::Object(mut object) => {
            object.entry("trace".to_string()).or_insert(trace);
            Value::Object(object)
        }
        other => json!({
            "detail": other,
            "trace": trace,
        }),
    }
}

pub fn publish_event(event: RuntimeEvent) {
    if let Some(store) = try_store() {
        let payload = normalize_payload(&event.kind, event.payload);
        let mut record = EventRecord::new(event.kind, payload);
        record.agent_id = event.agent_id;
        record.task_id = event.task_id;
        let _ = store.put_event(&record);
    }
}

pub fn root_task_ids(graph: &TaskGraph) -> HashSet<String> {
    graph
        .nodes
        .values()
        .filter(|node| node.depends_on.is_empty())
        .map(|node| node.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ready_tasks_ignore_blocked_dependencies() {
        let mut graph = TaskGraph::default();
        graph.add_task(TaskNode::new("plan", "plan"));
        graph.add_task(TaskNode::new("code", "code"));
        graph.add_task(TaskNode::new("test", "test"));
        graph.add_dependency("code", "plan");
        graph.add_dependency("test", "code");

        assert_eq!(graph.ready_tasks().len(), 1);
        assert_eq!(graph.ready_tasks()[0].id, "plan");

        graph.set_state("plan", TaskState::Completed);
        assert_eq!(graph.ready_tasks().len(), 1);
        assert_eq!(graph.ready_tasks()[0].id, "code");
    }

    #[test]
    fn blocked_tasks_include_failed_dependencies() {
        let mut graph = TaskGraph::default();
        graph.add_task(TaskNode::new("plan", "plan"));
        graph.add_task(TaskNode::new("code", "code"));
        graph.add_dependency("code", "plan");
        graph.set_state("plan", TaskState::Failed);
        assert_eq!(graph.blocked_tasks().len(), 1);
    }

    #[test]
    fn policy_decisions_capture_scope_and_mode() {
        let deny = PolicyDecision::deny(
            AutonomyMode::Assist,
            PolicyScope::Destructive("rm -rf".to_string()),
            "destructive command blocked",
        );
        assert!(!deny.allowed);
        assert_eq!(deny.mode, AutonomyMode::Assist);
    }

    #[test]
    fn root_tasks_detect_independent_nodes() {
        let mut graph = TaskGraph::default();
        graph.add_task(TaskNode::new("a", "a"));
        graph.add_task(TaskNode::new("b", "b"));
        graph.add_task(TaskNode::new("c", "c"));
        graph.add_dependency("c", "a");
        let roots = root_task_ids(&graph);
        assert!(roots.contains("a"));
        assert!(roots.contains("b"));
        assert!(!roots.contains("c"));
    }

    #[test]
    fn runtime_event_serializes() {
        let mut event = RuntimeEvent::new("task.created", json!({"task": "build"}));
        event.agent_id = Some("ag".into());
        event.task_id = Some("t1".into());
        let encoded = serde_json::to_string(&event).unwrap();
        assert!(encoded.contains("task.created"));
    }

    #[test]
    fn trace_envelope_parses_kind_into_fields() {
        let payload = normalize_payload("memory.search.started", json!({"query": "x"}));
        assert_eq!(payload["trace"]["kind"], "memory.search.started");
        assert_eq!(payload["trace"]["category"], "memory");
        assert_eq!(payload["trace"]["action"], "search");
        assert_eq!(payload["trace"]["stage"], "started");
    }

    #[test]
    fn scheduler_claims_ready_tasks_in_dependency_order() {
        let mut graph = TaskGraph::default();
        graph.add_task(TaskNode::new("plan", "plan"));
        graph.add_task(TaskNode::new("code", "code"));
        graph.add_task(TaskNode::new("test", "test"));
        graph.add_dependency("code", "plan");
        graph.add_dependency("test", "code");

        let mut scheduler = TaskScheduler::new(graph);
        assert_eq!(
            scheduler.next_ready().map(|task| task.id),
            Some("plan".into())
        );
        scheduler.mark_completed("plan");
        assert_eq!(
            scheduler.next_ready().map(|task| task.id),
            Some("code".into())
        );
        scheduler.mark_completed("code");
        assert_eq!(
            scheduler.next_ready().map(|task| task.id),
            Some("test".into())
        );
    }

    #[test]
    fn scheduler_roundtrips_through_disk() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("scheduler.json");

        let mut graph = TaskGraph::default();
        graph.add_task(TaskNode::new("plan", "plan"));
        let scheduler = TaskScheduler::new(graph);
        scheduler.save_to_path(&path).unwrap();

        let mut loaded = TaskScheduler::load_from_path(&path).unwrap();
        assert_eq!(loaded.graph().len(), 1);
        assert_eq!(loaded.next_ready().map(|task| task.id), Some("plan".into()));
    }

    #[test]
    fn resource_tracker_counts_usage_and_exhaustion() {
        let mut tracker = ResourceTracker::restrictive(2);
        assert!(tracker.can_start_tool());
        tracker.record_tool_call(10);
        assert!(tracker.can_start_tool());
        tracker.record_tool_call(20);
        assert!(!tracker.can_start_tool());
        assert!(tracker.is_exhausted());
        assert_eq!(tracker.usage.elapsed_ms, 30);
    }

    #[test]
    fn failure_classification_maps_common_messages() {
        assert_eq!(
            classify_failure("invalid params: missing task"),
            FailureClass::Validation
        );
        assert_eq!(
            classify_failure("permission denied: policy denied tool call"),
            FailureClass::Permission
        );
        assert_eq!(
            classify_failure("network timeout while contacting backend"),
            FailureClass::Transient
        );
        assert_eq!(
            repair_action_for(FailureClass::Transient, 1),
            RepairAction::Retry
        );
        assert_eq!(
            repair_action_for(FailureClass::Validation, 0),
            RepairAction::FixInput
        );
    }
}
