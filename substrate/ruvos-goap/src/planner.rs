//! A* GOAP planner — clean-room extraction from rUvnet ARCADIA
//! (`src/ai/goap.rs`), © Reuven Cohen / @ruvnet, MIT. `std` + `serde` only.

use crate::types::{GoapAction, GoapGoal, GoapPlan, PlanningStats, StateValue, WorldState};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, RwLock};

/// A node in the A* planning graph.
#[derive(Debug, Clone)]
struct PlanNode {
    world_state: WorldState,
    action: Option<GoapAction>,
    cost: f64,
    heuristic: f64,
    parent: Option<Box<PlanNode>>,
}

impl PlanNode {
    fn total_cost(&self) -> f64 {
        self.cost + self.heuristic
    }
}

impl PartialEq for PlanNode {
    fn eq(&self, other: &Self) -> bool {
        self.total_cost() == other.total_cost()
    }
}

impl Eq for PlanNode {}

impl PartialOrd for PlanNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PlanNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap behavior (BinaryHeap is a max-heap).
        other
            .total_cost()
            .partial_cmp(&self.total_cost())
            .unwrap_or(Ordering::Equal)
    }
}

/// GOAP planner using A* pathfinding over the action space.
pub struct GoapPlanner {
    actions: Arc<RwLock<Vec<GoapAction>>>,
    goals: Arc<RwLock<Vec<GoapGoal>>>,
    current_world_state: Arc<RwLock<WorldState>>,
    max_iterations: usize,
    stats: Arc<RwLock<PlanningStats>>,
}

impl GoapPlanner {
    pub fn new() -> Self {
        Self {
            actions: Arc::new(RwLock::new(Vec::new())),
            goals: Arc::new(RwLock::new(Vec::new())),
            current_world_state: Arc::new(RwLock::new(HashMap::new())),
            max_iterations: 1000,
            stats: Arc::new(RwLock::new(PlanningStats::default())),
        }
    }

    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Register a single action.
    pub fn add_action(&self, action: GoapAction) {
        self.actions.write().unwrap().push(action);
    }

    /// Register multiple actions.
    pub fn add_actions(&self, actions: Vec<GoapAction>) {
        self.actions.write().unwrap().extend(actions);
    }

    /// Register a goal (for `plan_best` / `select_goal`).
    pub fn add_goal(&self, goal: GoapGoal) {
        self.goals.write().unwrap().push(goal);
    }

    /// Replace the current world state.
    pub fn set_world_state(&self, state: WorldState) {
        *self.current_world_state.write().unwrap() = state;
    }

    /// Set one world-state value.
    pub fn update_state(&self, key: String, value: StateValue) {
        self.current_world_state.write().unwrap().insert(key, value);
    }

    /// Snapshot the current world state.
    pub fn get_world_state(&self) -> WorldState {
        self.current_world_state.read().unwrap().clone()
    }

    /// Statistics from the last planning run.
    pub fn get_stats(&self) -> PlanningStats {
        self.stats.read().unwrap().clone()
    }

    /// Select the highest-priority not-yet-satisfied goal.
    pub fn select_goal(&self) -> Option<GoapGoal> {
        let goals = self.goals.read().unwrap();
        let world_state = self.current_world_state.read().unwrap();

        goals
            .iter()
            .filter(|goal| !goal.is_satisfied(&world_state))
            .max_by(|a, b| {
                a.priority
                    .partial_cmp(&b.priority)
                    .unwrap_or(Ordering::Equal)
            })
            .cloned()
    }

    /// Plan a minimum-cost action sequence reaching `goal` via A*.
    pub fn plan(&self, goal: &GoapGoal) -> Option<GoapPlan> {
        let mut stats = PlanningStats::default();
        let world_state = self.current_world_state.read().unwrap().clone();

        // Early exit if the goal is already satisfied.
        if goal.is_satisfied(&world_state) {
            return Some(GoapPlan {
                actions: Vec::new(),
                total_cost: 0.0,
                goal_name: goal.name.clone(),
            });
        }

        let actions = self.actions.read().unwrap().clone();
        let mut open_set = BinaryHeap::new();
        let mut closed_set = HashSet::new();

        open_set.push(PlanNode {
            heuristic: goal.heuristic(&world_state),
            world_state,
            action: None,
            cost: 0.0,
            parent: None,
        });

        let mut iterations = 0;
        while let Some(current) = open_set.pop() {
            iterations += 1;
            stats.nodes_explored = iterations;
            if iterations > self.max_iterations {
                break;
            }

            if goal.is_satisfied(&current.world_state) {
                let plan = reconstruct_plan(current, goal.name.clone());
                stats.plan_length = plan.actions.len();
                stats.total_cost = plan.total_cost;
                *self.stats.write().unwrap() = stats;
                return Some(plan);
            }

            let state_hash = hash_world_state(&current.world_state);
            if !closed_set.insert(state_hash) {
                continue;
            }

            for action in &actions {
                if action.is_valid(&current.world_state) {
                    let new_state = action.apply(&current.world_state);
                    let new_cost = current.cost + action.cost;
                    let new_heuristic = goal.heuristic(&new_state);
                    open_set.push(PlanNode {
                        world_state: new_state,
                        action: Some(action.clone()),
                        cost: new_cost,
                        heuristic: new_heuristic,
                        parent: Some(Box::new(current.clone())),
                    });
                }
            }
        }

        *self.stats.write().unwrap() = stats;
        None
    }

    /// Plan for the highest-priority registered goal.
    pub fn plan_best(&self) -> Option<GoapPlan> {
        let goal = self.select_goal()?;
        self.plan(&goal)
    }

    /// Clear all actions, goals, world state, and stats.
    pub fn reset(&self) {
        self.actions.write().unwrap().clear();
        self.goals.write().unwrap().clear();
        self.current_world_state.write().unwrap().clear();
        *self.stats.write().unwrap() = PlanningStats::default();
    }
}

impl Default for GoapPlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Walk parent links back to the start, producing the forward action sequence.
fn reconstruct_plan(node: PlanNode, goal_name: String) -> GoapPlan {
    let total_cost = node.cost;
    let mut actions = VecDeque::new();
    let mut cur = node;
    while let Some(action) = cur.action.take() {
        actions.push_front(action);
        match cur.parent {
            Some(parent) => cur = *parent,
            None => break,
        }
    }
    GoapPlan {
        actions: actions.into_iter().collect(),
        total_cost,
        goal_name,
    }
}

/// Order-independent hash of a world state, for the A* closed set.
fn hash_world_state(state: &WorldState) -> String {
    use std::collections::BTreeMap;
    let sorted: BTreeMap<_, _> = state.iter().collect();
    format!("{:?}", sorted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_planning() {
        let planner = GoapPlanner::new();
        planner.add_action(
            GoapAction::new("pickup_weapon")
                .with_cost(1.0)
                .with_effect("has_weapon", StateValue::Bool(true)),
        );
        let goal =
            GoapGoal::new("be_armed", 1.0).with_condition("has_weapon", StateValue::Bool(true));

        let plan = planner.plan(&goal).expect("should find plan");
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].name, "pickup_weapon");
    }

    #[test]
    fn multi_step_planning_finds_ordered_sequence() {
        let planner = GoapPlanner::new();
        let go_to_armory = GoapAction::new("go_to_armory")
            .with_cost(2.0)
            .with_effect("at_armory", StateValue::Bool(true));
        let unlock_door = GoapAction::new("unlock_door")
            .with_cost(1.0)
            .with_precondition("at_armory", StateValue::Bool(true))
            .with_precondition("has_key", StateValue::Bool(true))
            .with_effect("door_unlocked", StateValue::Bool(true));
        let pickup = GoapAction::new("pickup_weapon")
            .with_cost(1.0)
            .with_precondition("at_armory", StateValue::Bool(true))
            .with_precondition("door_unlocked", StateValue::Bool(true))
            .with_effect("has_weapon", StateValue::Bool(true));
        planner.add_actions(vec![go_to_armory, unlock_door, pickup]);

        let mut initial = WorldState::new();
        initial.insert("has_key".to_string(), StateValue::Bool(true));
        planner.set_world_state(initial);

        let goal =
            GoapGoal::new("be_armed", 1.0).with_condition("has_weapon", StateValue::Bool(true));
        let plan = planner.plan(&goal).expect("should find plan");

        assert_eq!(plan.actions.len(), 3);
        assert_eq!(plan.actions[0].name, "go_to_armory");
        assert_eq!(plan.actions[1].name, "unlock_door");
        assert_eq!(plan.actions[2].name, "pickup_weapon");
    }

    #[test]
    fn unsolvable_goal_returns_none() {
        let planner = GoapPlanner::new();
        planner.add_action(GoapAction::new("noop").with_effect("x", StateValue::Bool(true)));
        let goal = GoapGoal::new("impossible", 1.0).with_condition("y", StateValue::Bool(true));
        assert!(planner.plan(&goal).is_none());
    }

    #[test]
    fn goal_selection_picks_highest_priority() {
        let planner = GoapPlanner::new();
        planner
            .add_goal(GoapGoal::new("low", 0.5).with_condition("state_a", StateValue::Bool(true)));
        planner
            .add_goal(GoapGoal::new("high", 0.9).with_condition("state_b", StateValue::Bool(true)));
        assert_eq!(planner.select_goal().unwrap().name, "high");
    }
}
