//! GOAP data model — clean-room extraction from rUvnet ARCADIA
//! (`src/ai/goap.rs`), © Reuven Cohen / @ruvnet, MIT. `std` + `serde` only.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// World-state key→value map representing conditions the planner reasons over.
pub type WorldState = HashMap<String, StateValue>;

/// State value types supporting various data representations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StateValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl StateValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            StateValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            StateValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            StateValue::Float(f) => Some(*f),
            _ => None,
        }
    }
}

/// A single operation that changes world state, with preconditions and effects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoapAction {
    pub name: String,
    pub cost: f64,
    pub preconditions: WorldState,
    pub effects: WorldState,
    pub metadata: HashMap<String, String>,
}

impl GoapAction {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cost: 1.0,
            preconditions: HashMap::new(),
            effects: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_cost(mut self, cost: f64) -> Self {
        self.cost = cost;
        self
    }

    pub fn with_precondition(mut self, key: impl Into<String>, value: StateValue) -> Self {
        self.preconditions.insert(key.into(), value);
        self
    }

    pub fn with_effect(mut self, key: impl Into<String>, value: StateValue) -> Self {
        self.effects.insert(key.into(), value);
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Check if this action's preconditions are met in the given world state.
    pub fn is_valid(&self, world_state: &WorldState) -> bool {
        self.preconditions
            .iter()
            .all(|(key, value)| world_state.get(key) == Some(value))
    }

    /// Apply this action's effects to a world state, returning the new state.
    pub fn apply(&self, world_state: &WorldState) -> WorldState {
        let mut new_state = world_state.clone();
        for (key, value) in &self.effects {
            new_state.insert(key.clone(), value.clone());
        }
        new_state
    }
}

/// A desired world state the planner tries to reach.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoapGoal {
    pub name: String,
    pub priority: f64,
    pub desired_state: WorldState,
    pub metadata: HashMap<String, String>,
}

impl GoapGoal {
    pub fn new(name: impl Into<String>, priority: f64) -> Self {
        Self {
            name: name.into(),
            priority,
            desired_state: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_condition(mut self, key: impl Into<String>, value: StateValue) -> Self {
        self.desired_state.insert(key.into(), value);
        self
    }

    /// Check if this goal is satisfied in the given world state.
    pub fn is_satisfied(&self, world_state: &WorldState) -> bool {
        self.desired_state
            .iter()
            .all(|(key, value)| world_state.get(key) == Some(value))
    }

    /// Heuristic distance to goal: number of unsatisfied conditions.
    pub fn heuristic(&self, world_state: &WorldState) -> f64 {
        self.desired_state
            .iter()
            .filter(|(key, value)| world_state.get(*key) != Some(*value))
            .count() as f64
    }
}

/// A planner result: the ordered action sequence and its total cost.
#[derive(Debug, Clone)]
pub struct GoapPlan {
    pub actions: Vec<GoapAction>,
    pub total_cost: f64,
    pub goal_name: String,
}

impl GoapPlan {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }
}

/// Statistics for a planning run.
#[derive(Debug, Clone, Default)]
pub struct PlanningStats {
    pub nodes_explored: usize,
    pub planning_time_ms: u128,
    pub plan_length: usize,
    pub total_cost: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_preconditions() {
        let action =
            GoapAction::new("test").with_precondition("has_weapon", StateValue::Bool(true));

        let mut state = WorldState::new();
        state.insert("has_weapon".to_string(), StateValue::Bool(true));
        assert!(action.is_valid(&state));

        state.insert("has_weapon".to_string(), StateValue::Bool(false));
        assert!(!action.is_valid(&state));
    }

    #[test]
    fn action_effects() {
        let action =
            GoapAction::new("pickup_weapon").with_effect("has_weapon", StateValue::Bool(true));

        let state = WorldState::new();
        let new_state = action.apply(&state);

        assert!(new_state.get("has_weapon").unwrap().as_bool().unwrap());
    }

    #[test]
    fn goal_satisfaction_and_heuristic() {
        let goal =
            GoapGoal::new("be_armed", 1.0).with_condition("has_weapon", StateValue::Bool(true));

        let mut state = WorldState::new();
        assert!(!goal.is_satisfied(&state));
        assert_eq!(goal.heuristic(&state), 1.0);

        state.insert("has_weapon".to_string(), StateValue::Bool(true));
        assert!(goal.is_satisfied(&state));
        assert_eq!(goal.heuristic(&state), 0.0);
    }
}
