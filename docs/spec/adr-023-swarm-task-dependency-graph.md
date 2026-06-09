# ADR-023: Task Dependency Graph in Swarm Coordination

**Status:** Proposed  
**Date:** 2026-06-09  
**Gap:** #19 in gap-register.md

## Context

`ruvos_swarm_assign` assigns tasks to agents but they are independent — there is no way to express that task B should not start until task A completes. For sprint pipelines this means the coordinator must manually poll task A's status before assigning B, adding orchestration boilerplate.

For ForgeCMS sprint pipelines the ordering is always: write tests → implement → run tests → code review. Without dependency tracking the coordinator must track this order explicitly in every sprint.

## Decision

Add `depends_on: [task_id, ...]` to the `ruvos_swarm_assign` input schema.

The swarm coordinator checks dependency status before dispatching any task:
- If all `depends_on` tasks are `completed`, the task is eligible for assignment
- If any dependency is `failed`, the dependent task is auto-failed with reason `dependency_failed: {task_id}`
- If any dependency is still `in_progress` or `pending`, the task is queued (not yet assigned)

`ruvos_swarm_status` response includes a `blocked_by` field per task showing which dependencies are outstanding.

Circular dependency detection: if adding a `depends_on` edge would create a cycle, `ruvos_swarm_assign` returns an error.

## Consequences

**Positive:**
- Sprint pipeline ordering (`write_tests → implement → run_tests → review`) is expressed declaratively, not manually polled
- Failed tasks propagate failure to dependents automatically — no orphaned in-progress agents waiting on a failed prerequisite
- Swarm status is now a true DAG that can be visualised

**Trade-offs:**
- Dependency evaluation runs on every heartbeat tick — O(tasks × dependencies) per tick, acceptable for sprint-scale swarms (<20 tasks)
- `depends_on` task IDs must already exist when the dependent task is assigned — requires upfront task registration before assignment

## Alternatives Considered

- **Coordinator polls manually**: current approach. Works but requires boilerplate in every pipeline. Rejected.
- **Event-driven via relay**: task A sends a relay message on completion; B listens. More decoupled but requires the relay bus to be running and adds latency. Deferred to daemon relay ADR.
