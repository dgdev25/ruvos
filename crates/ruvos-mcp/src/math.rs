//! Safe, checked arithmetic utilities for orchestration budgets and counters.
//!
//! All operations return `None` on overflow, underflow, or division by zero
//! rather than panicking or wrapping. Intended for coordinator agents that
//! track token budgets, task counts, and timeouts.

use std::num::NonZeroU64;

/// Result of a budget operation — carries the remaining value and whether the
/// configured ceiling was hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetResult {
    pub value: u64,
    pub capped: bool,
}

/// Adds two `i64` values, returning `None` on overflow or underflow.
#[inline]
pub fn safe_add(a: i64, b: i64) -> Option<i64> {
    a.checked_add(b)
}

/// Subtracts two `i64` values, returning `None` on overflow or underflow.
#[inline]
pub fn safe_sub(a: i64, b: i64) -> Option<i64> {
    a.checked_sub(b)
}

/// Multiplies two `i64` values, returning `None` on overflow.
#[inline]
pub fn safe_mul(a: i64, b: i64) -> Option<i64> {
    a.checked_mul(b)
}

/// Subtracts `spent` from `total` for unsigned counters; returns `None` on
/// underflow (spent > total).
#[inline]
pub fn checked_sub_u64(total: u64, spent: u64) -> Option<u64> {
    total.checked_sub(spent)
}

/// Multiplies two `u64` values (e.g. `tasks * tokens_per_task`), returning
/// `None` on overflow.
#[inline]
pub fn checked_mul_u64(a: u64, b: u64) -> Option<u64> {
    a.checked_mul(b)
}

/// Divides `numerator` by `denominator`.  The denominator is `NonZeroU64` so
/// division-by-zero is impossible at the type level.
#[inline]
pub fn checked_div_u64(numerator: u64, denominator: NonZeroU64) -> u64 {
    numerator / denominator
}

/// Computes the remaining budget (`total - spent`) capped at `ceiling`.
/// Returns zero (uncapped) when `spent > total` rather than wrapping.
pub fn budget_remaining(total: u64, spent: u64, ceiling: u64) -> BudgetResult {
    match total.checked_sub(spent) {
        Some(remaining) => {
            if remaining > ceiling {
                BudgetResult {
                    value: ceiling,
                    capped: true,
                }
            } else {
                BudgetResult {
                    value: remaining,
                    capped: false,
                }
            }
        }
        None => BudgetResult {
            value: 0,
            capped: false,
        },
    }
}

/// Scales `base_tokens` by `agent_count`, capped at `max_tokens`.
/// Returns `None` only if `base_tokens * agent_count` overflows `u64`.
pub fn scale_budget(base_tokens: u64, agent_count: u32, max_tokens: u64) -> Option<u64> {
    let scaled = base_tokens.checked_mul(u64::from(agent_count))?;
    Some(scaled.min(max_tokens))
}

/// Integer ceiling division: `⌈a / b⌉`.
pub fn ceil_div(a: u64, b: NonZeroU64) -> u64 {
    let b_val = b.get();
    a / b_val + u64::from(!a.is_multiple_of(b_val))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;

    fn nz(v: u64) -> NonZeroU64 {
        NonZeroU64::new(v).unwrap()
    }

    // ---- safe_add: happy path and boundaries --------------------------------

    #[test]
    fn add_basic_cases() {
        assert_eq!(safe_add(2, 3), Some(5)); // positive + positive
        assert_eq!(safe_add(-4, -6), Some(-10)); // both negative
        assert_eq!(safe_add(10, -3), Some(7)); // mixed, positive result
        assert_eq!(safe_add(-10, 3), Some(-7)); // mixed, negative result
        assert_eq!(safe_add(0, 0), Some(0)); // double zero
        assert_eq!(safe_add(0, 42), Some(42)); // zero identity left
        assert_eq!(safe_add(42, 0), Some(42)); // zero identity right
    }

    #[test]
    fn add_boundary_cases() {
        assert_eq!(safe_add(i64::MAX - 1, 1), Some(i64::MAX)); // exact upper bound
        assert_eq!(safe_add(i64::MIN + 1, -1), Some(i64::MIN)); // exact lower bound
        assert_eq!(safe_add(i64::MAX / 2, i64::MAX / 2), Some(i64::MAX - 1)); // large in-range
        assert_eq!(safe_add(i64::MAX, i64::MIN), Some(-1)); // max + min cancel
        assert_eq!(safe_add(i64::MAX, i64::MIN + 1), Some(0)); // near-perfect cancel
        assert_eq!(safe_add(i64::MAX, 0), Some(i64::MAX)); // identity at max
        assert_eq!(safe_add(i64::MIN, 0), Some(i64::MIN)); // identity at min
        assert_eq!(safe_add(i64::MAX, -1), Some(i64::MAX - 1)); // one step below max
        assert_eq!(safe_add(i64::MIN, 1), Some(i64::MIN + 1)); // one step above min
    }

    #[test]
    fn add_overflow_cases() {
        assert_eq!(safe_add(i64::MAX, 1), None); // one past max
        assert_eq!(safe_add(i64::MIN, -1), None); // one past min
        assert_eq!(safe_add(i64::MAX, 2), None); // two past max
        assert_eq!(safe_add(i64::MIN, -2), None); // two past min
        assert_eq!(safe_add(i64::MAX, i64::MAX), None); // double max
        assert_eq!(safe_add(i64::MIN, i64::MIN), None); // double min
                                                        // no wrapping — overflow is None, not Some(i64::MIN)
        assert_ne!(safe_add(i64::MAX, 1), Some(i64::MIN));
    }

    #[test]
    fn add_zero_identity_property() {
        for a in [0_i64, 1, -1, i64::MAX, i64::MIN, 42, -999] {
            assert_eq!(safe_add(a, 0), Some(a));
            assert_eq!(safe_add(0, a), Some(a));
        }
    }

    #[test]
    fn add_commutativity() {
        let pairs = [
            (0_i64, 0_i64),
            (1, -1),
            (i64::MAX, 0),
            (i64::MIN, 0),
            (i64::MAX, i64::MIN),
            (100, 200),
            (-50, 50),
            (123_456_789, -987_654_321),
        ];
        for (a, b) in pairs {
            assert_eq!(safe_add(a, b), safe_add(b, a), "({a}, {b})");
        }
    }

    #[test]
    fn add_i128_oracle() {
        let cases: &[(i64, i64)] = &[
            (0, 0),
            (1, 2),
            (-1, -2),
            (i64::MAX / 2, i64::MAX / 2),
            (i64::MAX - 1, 1),
            (i64::MIN + 1, -1),
            (i64::MAX, 1),
            (i64::MIN, -1),
            (i64::MAX, i64::MAX),
            (i64::MIN, i64::MIN),
            (i64::MAX, 2),
            (i64::MIN, -2),
        ];
        for &(a, b) in cases {
            let oracle = (a as i128) + (b as i128);
            let in_range = oracle >= i64::MIN as i128 && oracle <= i64::MAX as i128;
            if in_range {
                assert_eq!(safe_add(a, b), Some(oracle as i64), "({a}+{b})");
            } else {
                assert_eq!(safe_add(a, b), None, "({a}+{b}) should overflow");
            }
        }
    }

    // ---- safe_sub -----------------------------------------------------------

    #[test]
    fn sub_happy_path() {
        assert_eq!(safe_sub(10, 3), Some(7)); // basic
        assert_eq!(safe_sub(5, 5), Some(0)); // same value
        assert_eq!(safe_sub(0, 0), Some(0)); // double zero
        assert_eq!(safe_sub(5, -3), Some(8)); // subtract negative
        assert_eq!(safe_sub(-5, -3), Some(-2)); // both negative
        assert_eq!(safe_sub(-5, 3), Some(-8)); // larger magnitude
        assert_eq!(safe_sub(i64::MAX, 1), Some(i64::MAX - 1)); // one below max
        assert_eq!(safe_sub(i64::MIN, -1), Some(i64::MIN + 1)); // subtract -1 adds
        assert_eq!(safe_sub(i64::MIN + 1, 1), Some(i64::MIN)); // exact lower bound
    }

    #[test]
    fn sub_edge_cases() {
        assert_eq!(safe_sub(42, 0), Some(42)); // identity
        assert_eq!(safe_sub(i64::MAX, i64::MAX), Some(0)); // same large
        assert_eq!(safe_sub(i64::MIN, i64::MIN), Some(0)); // same large negative
                                                           // 0 - (MIN+1) = -(MIN+1) which fits because |MIN+1| = MAX
        assert_eq!(safe_sub(0, i64::MIN + 1), Some(i64::MAX));
    }

    #[test]
    fn sub_overflow_cases() {
        assert_eq!(safe_sub(i64::MIN, 1), None); // one past min
        assert_eq!(safe_sub(i64::MAX, -1), None); // add via sub overflows max
        assert_eq!(safe_sub(i64::MAX, i64::MIN), None); // widest span overflows
        assert_eq!(safe_sub(i64::MIN, i64::MAX), None); // negative span overflows
        assert_eq!(safe_sub(0, i64::MIN), None); // 0 - MIN = 2^63, > MAX
    }

    #[test]
    fn sub_i128_oracle() {
        let cases: &[(i64, i64)] = &[
            (10, 3),
            (5, 5),
            (0, 0),
            (5, -3),
            (-5, -3),
            (i64::MAX, 1),
            (i64::MIN, -1),
            (i64::MAX, i64::MIN),
            (0, i64::MIN),
        ];
        for &(a, b) in cases {
            let oracle = (a as i128) - (b as i128);
            let in_range = oracle >= i64::MIN as i128 && oracle <= i64::MAX as i128;
            if in_range {
                assert_eq!(safe_sub(a, b), Some(oracle as i64), "({a}-{b})");
            } else {
                assert_eq!(safe_sub(a, b), None, "({a}-{b}) should overflow");
            }
        }
    }

    // ---- safe_mul -----------------------------------------------------------

    #[test]
    fn mul_happy_path() {
        assert_eq!(safe_mul(6, 7), Some(42)); // basic
        assert_eq!(safe_mul(-3, 4), Some(-12)); // neg × pos
        assert_eq!(safe_mul(-3, -4), Some(12)); // neg × neg = pos
        assert_eq!(safe_mul(1, i64::MAX), Some(i64::MAX)); // identity left
        assert_eq!(safe_mul(i64::MAX, 1), Some(i64::MAX)); // identity right
        assert_eq!(safe_mul(-1, i64::MAX), Some(-i64::MAX)); // negate max (in range)
        assert_eq!(safe_mul(2, i64::MAX / 2), Some(i64::MAX - 1)); // large in-range
    }

    #[test]
    fn mul_zero_annihilator() {
        assert_eq!(safe_mul(0, i64::MAX), Some(0));
        assert_eq!(safe_mul(i64::MAX, 0), Some(0));
        assert_eq!(safe_mul(0, 0), Some(0));
        assert_eq!(safe_mul(1, 1), Some(1));
        assert_eq!(safe_mul(-1, 0), Some(0));
    }

    #[test]
    fn mul_overflow_cases() {
        assert_eq!(safe_mul(i64::MAX, 2), None); // double max
        assert_eq!(safe_mul(i64::MIN, 2), None); // double min
        assert_eq!(safe_mul(i64::MIN, -1), None); // negate MIN overflows (|MIN|>MAX)
        assert_eq!(safe_mul(-1, i64::MIN), None); // commuted
        assert_eq!(safe_mul(i64::MAX, i64::MAX), None); // square of max
    }

    #[test]
    fn mul_i128_oracle() {
        let cases: &[(i64, i64)] = &[
            (6, 7),
            (-3, 4),
            (-3, -4),
            (1, i64::MAX),
            (-1, i64::MAX),
            (2, i64::MAX / 2),
            (0, i64::MAX),
            (i64::MAX, 2),
            (i64::MIN, -1),
            (i64::MAX, i64::MAX),
        ];
        for &(a, b) in cases {
            let oracle = (a as i128) * (b as i128);
            let in_range = oracle >= i64::MIN as i128 && oracle <= i64::MAX as i128;
            if in_range {
                assert_eq!(safe_mul(a, b), Some(oracle as i64), "({a}*{b})");
            } else {
                assert_eq!(safe_mul(a, b), None, "({a}*{b}) should overflow");
            }
        }
    }

    // ---- checked_sub_u64 ----------------------------------------------------

    #[test]
    fn u64_sub_happy_path() {
        assert_eq!(checked_sub_u64(100, 40), Some(60));
        assert_eq!(checked_sub_u64(5, 5), Some(0)); // exact zero
        assert_eq!(checked_sub_u64(u64::MAX, 0), Some(u64::MAX)); // subtract zero
        assert_eq!(checked_sub_u64(u64::MAX, u64::MAX), Some(0)); // max - max
        assert_eq!(checked_sub_u64(u64::MAX, 1), Some(u64::MAX - 1)); // one below max
        assert_eq!(checked_sub_u64(0, 0), Some(0)); // double zero
    }

    #[test]
    fn u64_sub_underflow_cases() {
        assert_eq!(checked_sub_u64(3, 10), None); // spent > total
        assert_eq!(checked_sub_u64(0, 1), None); // underflow from zero
        assert_eq!(checked_sub_u64(0, u64::MAX), None); // maximum underflow
        assert_eq!(checked_sub_u64(u64::MAX - 1, u64::MAX), None); // off by one
    }

    // ---- checked_mul_u64 ----------------------------------------------------

    #[test]
    fn u64_mul_happy_path() {
        assert_eq!(checked_mul_u64(8, 7), Some(56));
        assert_eq!(checked_mul_u64(1, u64::MAX), Some(u64::MAX)); // identity left
        assert_eq!(checked_mul_u64(u64::MAX, 1), Some(u64::MAX)); // identity right
        assert_eq!(checked_mul_u64(2, u64::MAX / 2), Some(u64::MAX - 1)); // large in-range
        assert_eq!(checked_mul_u64(0, u64::MAX), Some(0)); // zero annihilator
        assert_eq!(checked_mul_u64(u64::MAX, 0), Some(0)); // commuted
        assert_eq!(checked_mul_u64(0, 0), Some(0)); // double zero
    }

    #[test]
    fn u64_mul_overflow_cases() {
        assert_eq!(checked_mul_u64(u64::MAX, 2), None);
        assert_eq!(checked_mul_u64(u64::MAX, u64::MAX), None);
        // u64::MAX / 2 + 1 is the first value that doubles over MAX
        assert_eq!(checked_mul_u64(2, u64::MAX / 2 + 1), None);
    }

    // ---- checked_div_u64 ----------------------------------------------------

    #[test]
    fn div_cases() {
        assert_eq!(checked_div_u64(20, nz(4)), 5); // exact
        assert_eq!(checked_div_u64(7, nz(2)), 3); // truncates toward zero
        assert_eq!(checked_div_u64(u64::MAX, nz(1)), u64::MAX); // identity (÷1)
        assert_eq!(checked_div_u64(u64::MAX, nz(u64::MAX)), 1); // divide by itself
        assert_eq!(checked_div_u64(0, nz(7)), 0); // zero numerator
        assert_eq!(checked_div_u64(1, nz(u64::MAX)), 0); // denom > numer → 0
        assert_eq!(checked_div_u64(u64::MAX, nz(2)), u64::MAX / 2); // large halved
        assert_eq!(checked_div_u64(100, nz(100)), 1); // equal values
        assert_eq!(checked_div_u64(100, nz(101)), 0); // denom one larger
    }

    #[test]
    fn div_zero_rejected_at_type_boundary() {
        // NonZeroU64::new(0) returns None — division by zero is compile-time impossible
        assert!(NonZeroU64::new(0).is_none());
    }

    // ---- budget_remaining ---------------------------------------------------

    #[test]
    fn budget_uncapped_cases() {
        let r = |t, s, c| budget_remaining(t, s, c);
        assert_eq!(
            r(1000, 300, 800),
            BudgetResult {
                value: 700,
                capped: false
            }
        ); // normal
        assert_eq!(
            r(1000, 999, 800),
            BudgetResult {
                value: 1,
                capped: false
            }
        ); // one left
        assert_eq!(
            r(1000, 1000, 800),
            BudgetResult {
                value: 0,
                capped: false
            }
        ); // fully spent
        assert_eq!(
            r(0, 0, 1000),
            BudgetResult {
                value: 0,
                capped: false
            }
        ); // zero total
           // remaining == ceiling uses `>` not `>=`, so NOT capped
        assert_eq!(
            r(1000, 200, 800),
            BudgetResult {
                value: 800,
                capped: false
            }
        ); // exact ==
    }

    #[test]
    fn budget_capped_cases() {
        let r = |t, s, c| budget_remaining(t, s, c);
        assert_eq!(
            r(5000, 0, 800),
            BudgetResult {
                value: 800,
                capped: true
            }
        ); // unspent > ceil
        assert_eq!(
            r(1000, 100, 500),
            BudgetResult {
                value: 500,
                capped: true
            }
        ); // remaining > ceil
        assert_eq!(
            r(u64::MAX, 0, 1000),
            BudgetResult {
                value: 1000,
                capped: true
            }
        ); // huge total
        assert_eq!(
            r(1000, 0, 0),
            BudgetResult {
                value: 0,
                capped: true
            }
        ); // zero ceiling
    }

    #[test]
    fn budget_underflow_saturation() {
        let r = |t, s, c| budget_remaining(t, s, c);
        // spent > total: saturates to 0, NOT capped
        assert_eq!(
            r(100, 200, 800),
            BudgetResult {
                value: 0,
                capped: false
            }
        );
        assert_eq!(
            r(0, u64::MAX, 1000),
            BudgetResult {
                value: 0,
                capped: false
            }
        ); // max underflow
        assert_eq!(
            r(1, 2, u64::MAX),
            BudgetResult {
                value: 0,
                capped: false
            }
        ); // large ceiling
        assert_eq!(
            r(0, 0, 0),
            BudgetResult {
                value: 0,
                capped: false
            }
        ); // all zeros
    }

    #[test]
    fn budget_result_is_copy_and_partial_eq() {
        let b = budget_remaining(100, 50, 200);
        let b2 = b; // Copy
        assert_eq!(b, b2);
    }

    // ---- scale_budget -------------------------------------------------------

    #[test]
    fn scale_happy_path() {
        assert_eq!(scale_budget(100, 3, 1000), Some(300)); // basic
        assert_eq!(scale_budget(100, 1, 1000), Some(100)); // single agent
        assert_eq!(scale_budget(1, u32::MAX, u64::MAX), Some(u32::MAX as u64)); // max agents
        assert_eq!(scale_budget(100, 5, 500), Some(500)); // exact match at ceiling
    }

    #[test]
    fn scale_capped_cases() {
        assert_eq!(scale_budget(100, 20, 500), Some(500)); // scaled > max
        assert_eq!(scale_budget(u64::MAX, 1, 1000), Some(1000)); // huge base, capped
        assert_eq!(scale_budget(1000, 1, 500), Some(500)); // single agent > max
    }

    #[test]
    fn scale_zero_cases() {
        assert_eq!(scale_budget(100, 0, 1000), Some(0)); // zero agents
        assert_eq!(scale_budget(0, 100, 1000), Some(0)); // zero base
        assert_eq!(scale_budget(0, 0, 0), Some(0)); // all zeros
    }

    #[test]
    fn scale_overflow_cases() {
        assert_eq!(scale_budget(u64::MAX, 2, u64::MAX), None);
        assert_eq!(scale_budget(u64::MAX / 2 + 1, 2, u64::MAX), None);
    }

    // ---- ceil_div -----------------------------------------------------------

    #[test]
    fn ceil_div_exact_cases() {
        assert_eq!(ceil_div(8, nz(4)), 2); // exact
        assert_eq!(ceil_div(0, nz(7)), 0); // zero numerator
        assert_eq!(ceil_div(u64::MAX, nz(1)), u64::MAX); // identity (÷1)
        assert_eq!(ceil_div(100, nz(100)), 1); // equal values
        assert_eq!(ceil_div(4, nz(2)), 2); // power of two
    }

    #[test]
    fn ceil_div_rounding_cases() {
        assert_eq!(ceil_div(9, nz(4)), 3); // remainder → rounds up
        assert_eq!(ceil_div(1, nz(1000)), 1); // tiny numerator, large denom
        assert_eq!(ceil_div(1001, nz(1000)), 2); // one over boundary
        assert_eq!(ceil_div(7, nz(3)), 3); // remainder 1
                                           // u64::MAX is odd, so MAX/2 has remainder 1 → rounds up
        assert_eq!(ceil_div(u64::MAX, nz(2)), u64::MAX / 2 + 1);
        assert_eq!(ceil_div(3, nz(10)), 1); // numer < denom → 1
        assert_eq!(ceil_div(1, nz(2)), 1); // smallest rounding case
    }

    #[test]
    fn ceil_div_boundary_cases() {
        assert_eq!(ceil_div(u64::MAX, nz(u64::MAX)), 1); // max/max = 1 exactly
        assert_eq!(ceil_div(u64::MAX - 1, nz(u64::MAX)), 1); // just under max denom
    }

    #[test]
    fn ceil_div_invariants() {
        // ⌈a/b⌉ ≥ ⌊a/b⌋  and  ⌈a/b⌉ * b ≥ a
        let cases: &[(u64, u64)] = &[
            (0, 1),
            (1, 1),
            (7, 3),
            (8, 4),
            (9, 4),
            (100, 7),
            (u64::MAX, 1),
            (u64::MAX, 2),
            (u64::MAX, u64::MAX),
        ];
        for &(a, b) in cases {
            let b_nz = nz(b);
            let result = ceil_div(a, b_nz);
            assert!(result >= a / b, "ceil >= floor failed for ({a},{b})");
            // result * b might overflow for huge values; use saturating to check
            let product = result.saturating_mul(b);
            assert!(product >= a, "result*b >= a failed for ({a},{b})");
        }
    }
}
