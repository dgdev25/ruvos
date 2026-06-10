use ruvos_mcp::math::{
    budget_remaining, ceil_div, checked_div_u64, checked_mul_u64, checked_sub_u64, safe_add,
    safe_mul, safe_sub, scale_budget, BudgetResult,
};
use std::num::NonZeroU64;

fn nz(v: u64) -> NonZeroU64 {
    NonZeroU64::new(v).unwrap()
}

// ── safe_sub ──────────────────────────────────────────────────────────────────

#[test]
fn sub_happy_path() {
    assert_eq!(safe_sub(0, 0), Some(0));
    assert_eq!(safe_sub(-3, -3), Some(0));
    assert_eq!(safe_sub(0, 5), Some(-5));
    assert_eq!(safe_sub(i64::MAX, i64::MAX), Some(0));
    assert_eq!(safe_sub(i64::MIN, i64::MIN), Some(0));
    assert_eq!(safe_sub(i64::MAX, 0), Some(i64::MAX));
    assert_eq!(safe_sub(i64::MIN, 0), Some(i64::MIN));
    assert_eq!(safe_sub(i64::MAX, 1), Some(i64::MAX - 1));
    assert_eq!(safe_sub(i64::MIN + 1, 1), Some(i64::MIN));
}

#[test]
fn sub_edge_cases() {
    assert_eq!(safe_sub(0, i64::MAX), Some(i64::MIN + 1));
    assert_eq!(safe_sub(100, -50), Some(150));
    assert_eq!(safe_sub(-100, 50), Some(-150));
}

#[test]
fn sub_overflows() {
    assert_eq!(safe_sub(i64::MIN, 2), None);
    assert_eq!(safe_sub(i64::MIN, i64::MAX), None);
    assert_eq!(safe_sub(i64::MAX, -1), None);
    assert_eq!(safe_sub(i64::MAX, i64::MIN), None);
    assert_eq!(safe_sub(i64::MAX, -2), None);
    assert_ne!(safe_sub(i64::MAX, 1), Some(i64::MIN));
}

// ── safe_mul ──────────────────────────────────────────────────────────────────

#[test]
fn mul_happy_path() {
    assert_eq!(safe_mul(i64::MIN, 0), Some(0));
    assert_eq!(safe_mul(0, 0), Some(0));
    assert_eq!(safe_mul(1, i64::MAX), Some(i64::MAX));
    assert_eq!(safe_mul(i64::MAX, 1), Some(i64::MAX));
    assert_eq!(safe_mul(i64::MIN, 1), Some(i64::MIN));
    assert_eq!(safe_mul(-1, -1), Some(1));
    assert_eq!(safe_mul(-1, i64::MAX), Some(-i64::MAX));
    assert_eq!(safe_mul(2, i64::MAX / 2), Some(i64::MAX - 1));
    assert_eq!(safe_mul(2, i64::MIN / 2), Some(i64::MIN));
}

#[test]
fn mul_commutativity() {
    for (a, b) in [(3_i64, -7_i64), (0, i64::MAX), (i64::MAX / 2, 2)] {
        assert_eq!(safe_mul(a, b), safe_mul(b, a), "commutativity ({a}, {b})");
    }
}

#[test]
fn mul_overflows() {
    assert_eq!(safe_mul(-1, i64::MIN), None);
    assert_eq!(safe_mul(i64::MIN, 2), None);
    assert_eq!(safe_mul(i64::MAX, i64::MAX), None);
    assert_eq!(safe_mul(i64::MIN, i64::MIN), None);
    assert_eq!(safe_mul(i64::MAX, -2), None);
    assert_eq!(safe_mul(2, i64::MAX / 2 + 1), None);
    assert_ne!(safe_mul(i64::MAX, 2), Some(-2));
}

// ── checked_sub_u64 ───────────────────────────────────────────────────────────

#[test]
fn u64_sub_happy_path() {
    assert_eq!(checked_sub_u64(0, 0), Some(0));
    assert_eq!(checked_sub_u64(u64::MAX, 0), Some(u64::MAX));
    assert_eq!(checked_sub_u64(u64::MAX, u64::MAX), Some(0));
    assert_eq!(checked_sub_u64(u64::MAX, 1), Some(u64::MAX - 1));
    assert_eq!(checked_sub_u64(1, 0), Some(1));
}

#[test]
fn u64_sub_underflows() {
    assert_eq!(checked_sub_u64(0, 1), None);
    assert_eq!(checked_sub_u64(0, u64::MAX), None);
    assert_eq!(checked_sub_u64(u64::MAX - 1, u64::MAX), None);
}

// ── checked_mul_u64 ───────────────────────────────────────────────────────────

#[test]
fn u64_mul_happy_path() {
    assert_eq!(checked_mul_u64(0, 0), Some(0));
    assert_eq!(checked_mul_u64(1, u64::MAX), Some(u64::MAX));
    assert_eq!(checked_mul_u64(u64::MAX, 1), Some(u64::MAX));
    assert_eq!(checked_mul_u64(2, u64::MAX / 2), Some(u64::MAX - 1));
}

#[test]
fn u64_mul_commutativity() {
    for (a, b) in [(3_u64, 5_u64), (0, u64::MAX), (u64::MAX / 2, 2)] {
        assert_eq!(checked_mul_u64(a, b), checked_mul_u64(b, a), "({a}, {b})");
    }
}

#[test]
fn u64_mul_overflows() {
    assert_eq!(checked_mul_u64(u64::MAX, u64::MAX), None);
    assert_eq!(checked_mul_u64(2, u64::MAX / 2 + 1), None);
}

// ── checked_div_u64 ───────────────────────────────────────────────────────────

#[test]
fn div_happy_path() {
    assert_eq!(checked_div_u64(0, nz(1)), 0);
    assert_eq!(checked_div_u64(0, nz(u64::MAX)), 0);
    assert_eq!(checked_div_u64(u64::MAX, nz(1)), u64::MAX);
    assert_eq!(checked_div_u64(u64::MAX, nz(u64::MAX)), 1);
    assert_eq!(checked_div_u64(1, nz(u64::MAX)), 0);
    assert_eq!(checked_div_u64(u64::MAX, nz(2)), u64::MAX / 2);
    assert_eq!(checked_div_u64(100, nz(100)), 1);
}

#[test]
fn div_matches_raw_division() {
    for (n, d) in [(20_u64, 4_u64), (7, 2), (0, 1), (u64::MAX, 3)] {
        assert_eq!(checked_div_u64(n, nz(d)), n / d);
    }
}

// ── budget_remaining ──────────────────────────────────────────────────────────

#[test]
fn budget_happy_uncapped() {
    assert_eq!(
        budget_remaining(0, 0, 0),
        BudgetResult {
            value: 0,
            capped: false
        }
    );
    assert_eq!(
        budget_remaining(100, 0, 200),
        BudgetResult {
            value: 100,
            capped: false
        }
    );
    assert_eq!(
        budget_remaining(100, 0, 100),
        BudgetResult {
            value: 100,
            capped: false
        }
    );
    assert_eq!(
        budget_remaining(u64::MAX, 0, u64::MAX),
        BudgetResult {
            value: u64::MAX,
            capped: false
        },
    );
    assert_eq!(
        budget_remaining(1000, 1, 999),
        BudgetResult {
            value: 999,
            capped: false
        }
    );
    assert_eq!(
        budget_remaining(1000, 1, 1000),
        BudgetResult {
            value: 999,
            capped: false
        }
    );
}

#[test]
fn budget_capped_cases() {
    assert_eq!(
        budget_remaining(1000, 0, 500),
        BudgetResult {
            value: 500,
            capped: true
        }
    );
    assert_eq!(
        budget_remaining(1000, 0, 0),
        BudgetResult {
            value: 0,
            capped: true
        }
    );
    assert_eq!(
        budget_remaining(1000, 1, 998),
        BudgetResult {
            value: 998,
            capped: true
        }
    );
}

#[test]
fn budget_overspent() {
    assert_eq!(
        budget_remaining(0, 1, 1000),
        BudgetResult {
            value: 0,
            capped: false
        }
    );
    assert_eq!(
        budget_remaining(50, u64::MAX, 9999),
        BudgetResult {
            value: 0,
            capped: false
        }
    );
    assert_eq!(
        budget_remaining(u64::MAX - 1, u64::MAX, 1000),
        BudgetResult {
            value: 0,
            capped: false
        },
    );
}

#[test]
fn budget_capped_invariant() {
    for (total, spent, ceiling) in [
        (5000_u64, 0_u64, 800_u64),
        (1000, 0, 500),
        (1000, 0, 0),
        (1000, 1, 998),
    ] {
        let r = budget_remaining(total, spent, ceiling);
        if r.capped {
            assert_eq!(
                r.value, ceiling,
                "capped value≠ceiling ({total},{spent},{ceiling})"
            );
        }
    }
}

// ── scale_budget ──────────────────────────────────────────────────────────────

#[test]
fn scale_happy_path() {
    assert_eq!(scale_budget(0, 100, 1000), Some(0));
    assert_eq!(scale_budget(1, 1, u64::MAX), Some(1));
    assert_eq!(scale_budget(u64::MAX, 1, u64::MAX), Some(u64::MAX));
}

#[test]
fn scale_capped_cases() {
    assert_eq!(scale_budget(1000, 10, 1), Some(1));
    assert_eq!(scale_budget(u64::MAX / 2, 2, 100), Some(100));
    assert_eq!(scale_budget(100, u32::MAX, 1_000_000), Some(1_000_000));
}

#[test]
fn scale_overflow_cases() {
    assert_eq!(scale_budget(u64::MAX / 2 + 1, 2, u64::MAX), None);
    assert_eq!(scale_budget(u64::MAX, u32::MAX, u64::MAX), None);
}

#[test]
fn scale_never_exceeds_max() {
    for (base, count, max) in [
        (100_u64, 3_u32, 1000_u64),
        (100, 20, 500),
        (0, 100, 1000),
        (u64::MAX, 1, u64::MAX),
    ] {
        if let Some(v) = scale_budget(base, count, max) {
            assert!(v <= max, "scale({base},{count},{max})={v} exceeded max");
        }
    }
}

// ── ceil_div ──────────────────────────────────────────────────────────────────

#[test]
fn ceil_div_exact_cases() {
    assert_eq!(ceil_div(9, nz(3)), 3);
    assert_eq!(ceil_div(u64::MAX, nz(1)), u64::MAX);
    assert_eq!(ceil_div(u64::MAX, nz(u64::MAX)), 1);
    assert_eq!(ceil_div(100, nz(100)), 1);
    assert_eq!(ceil_div(0, nz(u64::MAX)), 0);
}

#[test]
fn ceil_div_rounding_cases() {
    assert_eq!(ceil_div(1, nz(2)), 1);
    assert_eq!(ceil_div(3, nz(2)), 2);
    assert_eq!(ceil_div(u64::MAX - 1, nz(u64::MAX)), 1);
    assert_eq!(ceil_div(u64::MAX, nz(2)), u64::MAX / 2 + 1);
    assert_eq!(ceil_div(101, nz(100)), 2);
}

#[test]
fn ceil_div_at_least_floor_div() {
    for (a, b) in [(9_u64, 4_u64), (8, 4), (0, 7), (u64::MAX, 3), (1, u64::MAX)] {
        assert!(ceil_div(a, nz(b)) >= a / b, "ceil({a}/{b}) < floor");
    }
}

#[test]
fn ceil_div_covers_numerator() {
    for (a, b) in [(9_u64, 4_u64), (8, 4), (1, 1000), (u64::MAX - 1, u64::MAX)] {
        let c = ceil_div(a, nz(b));
        assert!(c.saturating_mul(b) >= a, "ceil({a}/{b})={c}: {c}*{b} < {a}");
    }
}

// ── cross-function invariants ─────────────────────────────────────────────────

#[test]
fn sub_agrees_with_add_negated() {
    for (a, b) in [(10_i64, 3_i64), (0, 0), (-5, -5), (100, -50)] {
        if let Some(neg_b) = b.checked_neg() {
            assert_eq!(safe_sub(a, b), safe_add(a, neg_b), "({a},{b})");
        }
    }
}

#[test]
fn budget_result_is_copy_and_debug() {
    let r = BudgetResult {
        value: 42,
        capped: false,
    };
    let r2 = r;
    assert_eq!(r, r2);
    let _ = format!("{r:?}");
}
