//! # ruvos-goap — Goal-Oriented Action Planning (A*)
//!
//! Clean-room extraction of the GOAP engine from rUvnet **ARCADIA**
//! (`src/ai/goap.rs`), © Reuven Cohen / [@ruvnet](https://github.com/ruvnet), MIT.
//! Depends only on `std` + `serde` — none of ARCADIA's runtime deps.
//!
//! Model a problem as a set of [`GoapAction`]s (each with `preconditions` and
//! `effects` over a [`WorldState`]) plus a [`GoapGoal`] (a desired end-state);
//! [`GoapPlanner::plan`] runs A* to find the minimum-cost action sequence that
//! reaches the goal.
//!
//! ```
//! use ruvos_goap::{GoapAction, GoapGoal, GoapPlanner, StateValue};
//! let planner = GoapPlanner::new();
//! planner.add_action(
//!     GoapAction::new("pickup").with_effect("armed", StateValue::Bool(true)),
//! );
//! let goal = GoapGoal::new("be_armed", 1.0).with_condition("armed", StateValue::Bool(true));
//! let plan = planner.plan(&goal).expect("a plan");
//! assert_eq!(plan.actions[0].name, "pickup");
//! ```

pub mod planner;
pub mod types;

pub use planner::GoapPlanner;
pub use types::{GoapAction, GoapGoal, GoapPlan, PlanningStats, StateValue, WorldState};
