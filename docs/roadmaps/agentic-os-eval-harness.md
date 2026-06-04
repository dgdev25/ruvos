# Agentic OS Evaluation Harness

This harness measures whether rUvOS actually behaves like an agentic runtime:
it should route tasks correctly, recover from failure, persist artifacts, and
produce replayable traces that are useful to humans.

## Goals

- Detect orchestration regressions early.
- Measure handoff quality across `planner -> coder -> tester -> reviewer`.
- Measure swarm recovery behavior under stale membership and task rebalance.
- Measure skill selection quality and whether selected bundles improve output.
- Keep the harness deterministic enough for CI, but realistic enough to catch
  integration failures.

## Primary metrics

- task success rate
- step completion rate
- failure recovery rate
- artifact persistence rate
- replay completeness
- skill selection hit rate
- handoff fidelity
- manual intervention rate

## First benchmark tasks

### 1. Orchestration handoff benchmark

**Scenario**
- Input: a small code-writing task that should produce an actual Rust snippet.
- Pipeline: planner -> coder -> tester -> reviewer.
- Expectation: each step consumes the previous step’s artifact and produces a
  better final result than a generic plan-only output.

**Checks**
- planner output exists
- coder output contains executable Rust
- tester output references the coder artifact
- reviewer output references the tester artifact
- final run persists selected skills and replayable traces

**Success criteria**
- end-to-end run succeeds
- artifact chain is complete
- output is source code, not only prose

### 2. Swarm recovery benchmark

**Scenario**
- Input: a swarm with one stale member, one live member, and a rebalanceable
  task queue.
- Action: assign work, age out a member, rebalance, then complete the swarm.

**Checks**
- stale member is detected
- work is reassigned
- health score improves after rebalance
- completion/failure state is persisted

**Success criteria**
- rebalance moves work off the stale member
- no task is left unowned at the end

### 3. Skill-routing benchmark

**Scenario**
- Input: a task that clearly needs a specific capability bundle, such as Rust
  safety plus testing.
- Action: select the run-level bundle and use it across the orchestration.

**Checks**
- selected bundle is deterministic for the same input
- bundle is persisted to disk
- the chosen skills appear in step metadata
- feedback is recorded on completion

**Success criteria**
- selected skills are consistent across runs
- the same run reuses the same bundle for all steps
- feedback counters update after success/failure

### 4. Swarm learning-loop benchmark

**Scenario**
- Input: repeat the same swarm task several times with different outcomes
  (success, failure, rebalance recovery).
- Action: record the swarm’s metrics, replay trace, and outcome signal after
  each run.
- Expectation: the next run should use the recorded feedback to improve one
  policy decision, such as topology inference, member assignment preference, or
  rebalance timing.

**Checks**
- the run emits a durable outcome signal
- replay artifacts exist for every run
- swarm metrics are persisted
- the selected policy changes after feedback is recorded
- the change is explainable in the audit trail

**Success criteria**
- the swarm makes a different, better decision on the next run
- the decision change is traceable to prior feedback
- the policy update does not break determinism for identical inputs when no
  feedback has been recorded

## Harness shape

- Start with deterministic fixtures and unit/integration-style scenarios.
- Promote the stable cases into CI.
- Keep benchmark outputs in a machine-readable JSON format.
- Reuse `gov.replay` and `gov.report` as the evidence layer.

## Suggested execution order

1. Orchestration handoff benchmark
2. Swarm recovery benchmark
3. Skill-routing benchmark
4. Swarm learning-loop benchmark

## Non-goals

- training or fine-tuning
- full load testing
- synthetic speed claims without replayable evidence
