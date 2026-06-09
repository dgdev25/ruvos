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
                BudgetResult { value: ceiling, capped: true }
            } else {
                BudgetResult { value: remaining, capped: false }
            }
        }
        None => BudgetResult { value: 0, capped: false },
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
    a / b_val + u64::from(a % b_val != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;

    fn nz(v: u64) -> NonZeroU64 {
        NonZeroU64::new(v).unwrap()
    }

    // --- safe_add ---

    #[test]
    fn adds_positive_numbers() {
        assert_eq!(safe_add(2, 3), Some(5));
    }

    #[test]
    fn adds_negative_numbers() {
        assert_eq!(safe_add(-4, -6), Some(-10));
    }

    #[test]
    fn adds_mixed_signs() {
        assert_eq!(safe_add(10, -3), Some(7));
    }

    #[test]
    fn add_mixed_signs_negative_result() {
        assert_eq!(safe_add(-10, 3), Some(-7));
    }

    #[test]
    fn adds_zeros() {
        assert_eq!(safe_add(0, 0), Some(0));
    }

    #[test]
    fn add_zero_plus_positive() {
        assert_eq!(safe_add(0, 42), Some(42));
    }

    #[test]
    fn add_zero_plus_negative() {
        assert_eq!(safe_add(0, -42), Some(-42));
    }

    #[test]
    fn add_positive_plus_zero() {
        assert_eq!(safe_add(42, 0), Some(42));
    }

    #[test]
    fn add_large_positives_within_range() {
        assert_eq!(safe_add(i64::MAX / 2, i64::MAX / 2), Some(i64::MAX - 1));
    }

    #[test]
    fn add_max_plus_zero() {
        assert_eq!(safe_add(i64::MAX, 0), Some(i64::MAX));
    }

    #[test]
    fn add_min_plus_zero() {
        assert_eq!(safe_add(i64::MIN, 0), Some(i64::MIN));
    }

    #[test]
    fn add_opposites_cancel_to_zero() {
        assert_eq!(safe_add(i64::MAX, i64::MIN + 1), Some(0));
    }

    #[test]
    fn add_result_equals_max() {
        assert_eq!(safe_add(i64::MAX - 1, 1), Some(i64::MAX));
    }

    #[test]
    fn add_result_equals_min() {
        assert_eq!(safe_add(i64::MIN + 1, -1), Some(i64::MIN));
    }

    #[test]
    fn add_commutative() {
        let (a, b) = (123_456_789_i64, -987_654_321_i64);
        assert_eq!(safe_add(a, b), safe_add(b, a));
    }

    #[test]
    fn add_max_minus_one() {
        assert_eq!(safe_add(i64::MAX, -1), Some(i64::MAX - 1));
    }

    #[test]
    fn add_min_plus_one() {
        assert_eq!(safe_add(i64::MIN, 1), Some(i64::MIN + 1));
    }

    #[test]
    fn add_both_max_overflows() {
        assert_eq!(safe_add(i64::MAX, i64::MAX), None);
    }

    #[test]
    fn add_both_min_underflows() {
        assert_eq!(safe_add(i64::MIN, i64::MIN), None);
    }

    #[test]
    fn add_max_plus_min() {
        assert_eq!(safe_add(i64::MAX, i64::MIN), Some(-1));
    }

    #[test]
    fn add_near_cancellation_stays_safe() {
        assert_eq!(safe_add(i64::MAX / 2 + 1, -(i64::MAX / 2)), Some(1));
    }

    #[test]
    fn overflow_returns_none() {
        assert_eq!(safe_add(i64::MAX, 1), None);
    }

    #[test]
    fn underflow_returns_none() {
        assert_eq!(safe_add(i64::MIN, -1), None);
    }

    #[test]
    fn add_overflow_by_two() {
        assert_eq!(safe_add(i64::MAX, 2), None);
    }

    #[test]
    fn add_underflow_by_two() {
        assert_eq!(safe_add(i64::MIN, -2), None);
    }

    #[test]
    fn add_overflow_returns_none_not_wrapped() {
        assert_ne!(safe_add(i64::MAX, 1), Some(i64::MIN));
        assert_eq!(safe_add(i64::MAX, 1), None);
    }

    #[test]
    fn add_zero_identity_property() {
        for a in [0_i64, 1, -1, i64::MAX, i64::MIN, 42, -999] {
            assert_eq!(safe_add(a, 0), Some(a));
            assert_eq!(safe_add(0, a), Some(a));
        }
    }

    #[test]
    fn add_i128_oracle_some_matches() {
        let pairs = [
            (0_i64, 0_i64),
            (1, 2),
            (-1, -2),
            (i64::MAX / 2, i64::MAX / 2),
            (i64::MAX - 1, 1),
            (i64::MIN + 1, -1),
        ];
        for (a, b) in pairs {
            let oracle = (a as i128) + (b as i128);
            let result = safe_add(a, b);
            assert!(result.is_some(), "expected Some for ({a}, {b})");
            assert_eq!(result.unwrap() as i128, oracle);
        }
    }

    #[test]
    fn add_i128_oracle_none_matches() {
        let pairs = [
            (i64::MAX, 1_i64),
            (i64::MIN, -1),
            (i64::MAX, i64::MAX),
            (i64::MIN, i64::MIN),
            (i64::MAX, 2),
            (i64::MIN, -2),
        ];
        for (a, b) in pairs {
            let oracle = (a as i128) + (b as i128);
            let in_range = oracle >= i64::MIN as i128 && oracle <= i64::MAX as i128;
            assert!(!in_range, "test case ({a}, {b}) should overflow");
            assert_eq!(safe_add(a, b), None);
        }
    }

    #[test]
    fn add_commutativity_sample_set() {
        let pairs = [
            (0_i64, 0_i64),
            (1, -1),
            (i64::MAX, 0),
            (i64::MIN, 0),
            (i64::MAX, i64::MIN),
            (100, 200),
            (-50, 50),
        ];
        for (a, b) in pairs {
            assert_eq!(safe_add(a, b), safe_add(b, a), "commutativity failed for ({a}, {b})");
        }
    }

    // --- safe_sub ---

    #[test]
    fn sub_normal() {
        assert_eq!(safe_sub(10, 3), Some(7));
    }

    #[test]
    fn sub_to_zero() {
        assert_eq!(safe_sub(5, 5), Some(0));
    }

    #[test]
    fn sub_overflow_returns_none() {
        assert_eq!(safe_sub(i64::MIN, 1), None);
    }

    // --- safe_mul ---

    #[test]
    fn mul_normal() {
        assert_eq!(safe_mul(6, 7), Some(42));
    }

    #[test]
    fn mul_by_zero() {
        assert_eq!(safe_mul(i64::MAX, 0), Some(0));
    }

    #[test]
    fn mul_overflow_returns_none() {
        assert_eq!(safe_mul(i64::MAX, 2), None);
    }

    // --- checked_sub_u64 ---

    #[test]
    fn u64_sub_normal() {
        assert_eq!(checked_sub_u64(100, 40), Some(60));
    }

    #[test]
    fn u64_sub_exact_zero() {
        assert_eq!(checked_sub_u64(5, 5), Some(0));
    }

    #[test]
    fn u64_sub_underflow_returns_none() {
        assert_eq!(checked_sub_u64(3, 10), None);
    }

    // --- checked_mul_u64 ---

    #[test]
    fn u64_mul_normal() {
        assert_eq!(checked_mul_u64(8, 7), Some(56));
    }

    #[test]
    fn u64_mul_by_zero() {
        assert_eq!(checked_mul_u64(u64::MAX, 0), Some(0));
    }

    #[test]
    fn u64_mul_overflow_returns_none() {
        assert_eq!(checked_mul_u64(u64::MAX, 2), None);
    }

    // --- checked_div_u64 ---

    #[test]
    fn div_exact() {
        assert_eq!(checked_div_u64(20, nz(4)), 5);
    }

    #[test]
    fn div_truncates() {
        assert_eq!(checked_div_u64(7, nz(2)), 3);
    }

    // --- budget_remaining ---

    #[test]
    fn budget_within_ceiling() {
        let r = budget_remaining(1000, 300, 800);
        assert_eq!(r, BudgetResult { value: 700, capped: false });
    }

    #[test]
    fn budget_capped_at_ceiling() {
        let r = budget_remaining(5000, 0, 800);
        assert_eq!(r, BudgetResult { value: 800, capped: true });
    }

    #[test]
    fn budget_spent_exceeds_total() {
        let r = budget_remaining(100, 200, 800);
        assert_eq!(r, BudgetResult { value: 0, capped: false });
    }

    #[test]
    fn budget_fully_spent() {
        let r = budget_remaining(500, 500, 1000);
        assert_eq!(r, BudgetResult { value: 0, capped: false });
    }

    // --- scale_budget ---

    #[test]
    fn scale_below_max() {
        assert_eq!(scale_budget(100, 3, 1000), Some(300));
    }

    #[test]
    fn scale_capped_at_max() {
        assert_eq!(scale_budget(100, 20, 500), Some(500));
    }

    #[test]
    fn scale_overflow_returns_none() {
        assert_eq!(scale_budget(u64::MAX, 2, u64::MAX), None);
    }

    #[test]
    fn scale_zero_agents() {
        assert_eq!(scale_budget(100, 0, 1000), Some(0));
    }

    // --- ceil_div ---

    #[test]
    fn ceil_div_exact() {
        assert_eq!(ceil_div(8, nz(4)), 2);
    }

    #[test]
    fn ceil_div_rounds_up() {
        assert_eq!(ceil_div(9, nz(4)), 3);
    }

    #[test]
    fn ceil_div_one() {
        assert_eq!(ceil_div(1, nz(1000)), 1);
    }

    #[test]
    fn ceil_div_zero_numerator() {
        assert_eq!(ceil_div(0, nz(7)), 0);
    }
}
