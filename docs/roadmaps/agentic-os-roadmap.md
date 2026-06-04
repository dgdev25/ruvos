# Agentic OS Roadmap

This roadmap turns rUvOS from a tool runner into an agentic operating system.
Each phase is deliberately small enough to ship, validate, and roll forward
without losing the existing MCP surface.

## Progress Tracker

- [x] Runtime event bus and trace envelope
- [x] Tool/session/agent/hook/relay/orchestration/retrieval instrumentation
- [x] Roadmap + phase ADR set
- [x] Common retrieval trace schema
- [x] Tool-call policy engine
- [x] Task graph scheduler for durable jobs
- [x] Autonomy modes wired into execution
- [x] Resource budgets and worktree sandboxes
- [x] Failure-classification repair loops
- [x] Repo intelligence and intent memory
- [x] Coordination contracts between agents
- [x] Replayable session traces and governance reports

## Phase 1: Runtime Spine

- Add a unified event bus for tool calls, agent actions, hooks, retries, and artifacts.
- Add a task graph scheduler that can own sessions, agents, relay messages, and follow-up work.
- Add a policy engine for tool, file, network, and destructive-action permissions.

## Phase 2: Safe Autonomy

- Add explicit autonomy modes: `manual`, `assist`, `delegate`, `autopilot`.
- Add durable background jobs with pause, resume, and retry semantics.
- Add checkpointing so long runs can survive restarts.

## Phase 3: Resource Control

- Add per-task budgets for time, tokens, tool usage, file scope, and retry counts.
- Add worktree orchestration so agents can run in isolated git sandboxes.
- Add safe merge and handoff with provenance.

## Phase 4: Self-Healing

- Add failure classification and automatic repair loops.
- Route failed runs into recovery plans instead of only reporting failure.
- Learn which recovery strategies worked and prefer them later.

## Phase 5: System Intelligence

- Add repo intelligence: architecture maps, ownership graphs, hot paths, test gaps, and dependency risk.
- Add intent memory for stable goals, preferences, and recurring workflows.
- Add periodic synthesis jobs that turn repeated actions into docs, templates, or policies.

## Phase 6: Coordination Layer

- Upgrade relay from messaging to structured collaboration contracts between agents and instances.
- Add role ownership, handoff rules, and conflict resolution for multi-agent work.
- Add a visible system-state view for sessions, agents, goals, blockers, and health.

## Phase 7: Evaluation and Governance

- Add replayable session traces so every run can be reconstructed from events and artifacts.
- Add workflow benchmarks: success rate, time-to-completion, intervention rate, wasted context, rollback rate.
- Add policy audits and operator-visible governance reports.
