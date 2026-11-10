//! Field validation for collections. Every failing field is reported, not just the first.

use std::collections::BTreeMap;
use std::fmt;

/// Longest allowed collection name, in characters.
pub const MAX_NAME_LEN: usize = 64;

/// Largest block size for `block` durability.
pub const MAX_BLOCK_SIZE: u32 = 1_000_000;

/// Validation failures keyed by field name (`"name"`, `"max"`, …), with a
/// human-readable problem per field.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ValidationErrors {
    fields: BTreeMap<String, String>,
}

impl ValidationErrors {
    /// A single failing field.
    #[must_use]
    pub fn field(field: &str, problem: impl Into<String>) -> Self {
        Self::default().with(field, problem)
    }

    /// A copy with one more failing field. The first problem per field is kept.
    #[must_use]
    pub fn with(self, field: &str, problem: impl Into<String>) -> Self {
        let mut fields = self.fields;
        fields
            .entry(field.to_owned())
            .or_insert_with(|| problem.into());
        Self { fields }
    }

    /// A copy with `problem` added only when `failed` is true.
    #[must_use]
    pub fn check(self, failed: bool, field: &str, problem: impl Into<String>) -> Self {
        if failed {
            self.with(field, problem)
        } else {
            self
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    #[must_use]
    pub fn fields(&self) -> &BTreeMap<String, String> {
        &self.fields
    }

    /// `Ok(value)` when nothing failed, else `Err(self)`.
    ///
    /// # Errors
    ///
    /// `self` when at least one field failed.
    pub fn into_result<T>(self, value: T) -> Result<T, Self> {
        if self.is_empty() {
            Ok(value)
        } else {
            Err(self)
        }
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self
            .fields
            .iter()
            .map(|(field, problem)| format!("{field}: {problem}"))
            .collect();
        write!(f, "validation failed ({})", parts.join("; "))
    }
}

impl std::error::Error for ValidationErrors {}

/// Why `name` is not a valid collection name, or `None` if it is.
///
/// Rules: 1–64 characters, `^[a-z0-9][a-z0-9_.-]*$`.
#[must_use]
pub fn name_problem(name: &str) -> Option<&'static str> {
    let Some(first) = name.chars().next() else {
        return Some("must not be empty");
    };
    if name.chars().count() > MAX_NAME_LEN {
        return Some("must be at most 64 characters");
    }
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Some("must start with a lowercase letter or digit");
    }
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "_.-".contains(c);
    if !name.chars().all(allowed) {
        return Some("may only contain a-z, 0-9, '_', '.' and '-'");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_names() {
        for name in ["a", "0", "orders", "eu.orders-2026_q1", &"a".repeat(64)] {
            assert_eq!(name_problem(name), None, "{name}");
        }
    }

    #[test]
    fn rejects_invalid_names_with_a_reason() {
        let cases = [
            ("", "must not be empty"),
            (&*"a".repeat(65), "must be at most 64 characters"),
            ("Orders", "must start with a lowercase letter or digit"),
            ("_orders", "must start with a lowercase letter or digit"),
            ("orders!", "may only contain a-z, 0-9, '_', '.' and '-'"),
            ("ordérs", "may only contain a-z, 0-9, '_', '.' and '-'"),
            ("or ders", "may only contain a-z, 0-9, '_', '.' and '-'"),
        ];
        for (name, problem) in cases {
            assert_eq!(name_problem(name), Some(problem), "{name}");
        }
    }

    #[test]
    fn collects_every_failing_field_and_keeps_the_first_problem() {
        let errors = ValidationErrors::default()
            .with("name", "bad")
            .check(true, "max", "too small")
            .check(false, "step", "never added")
            .with("name", "second problem is ignored");

        assert_eq!(errors.fields().len(), 2);
        assert_eq!(errors.fields()["name"], "bad");
        assert_eq!(
            errors.to_string(),
            "validation failed (max: too small; name: bad)"
        );
        assert_eq!(errors.clone().into_result(()), Err(errors));
        assert_eq!(ValidationErrors::default().into_result(7), Ok(7));
    }
}
