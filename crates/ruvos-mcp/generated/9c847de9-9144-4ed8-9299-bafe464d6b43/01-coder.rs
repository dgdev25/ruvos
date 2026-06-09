pub fn safe_add(left: i32, right: i32) -> Option<i32> {
left.checked_add(right)
}

#[cfg(test)]
mod tests {
use super::safe_add;

#[test]
fn adds_small_numbers() {
assert_eq!(safe_add(2, 3), Some(5));
}

#[test]
fn rejects_overflow() {
assert_eq!(safe_add(i32::MAX, 1), None);
}
}
