//! The collection model and the pure sequence arithmetic shared by every backend.
//!
//! Values are immutable: every change (`advance`, `apply`, `reset`) returns a new
//! [`Collection`]. Backends only add atomicity and durability.

use std::num::{NonZeroU32, NonZeroU64};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::validation::{MAX_BLOCK_SIZE, ValidationErrors, name_problem};

/// A sequence value. Signed 64-bit (ISSUE-0002).
pub type Id = i64;

/// Default first ID of a new collection.
pub const DEFAULT_START: Id = 1;

/// Default distance between consecutive IDs.
pub const DEFAULT_STEP: NonZeroU64 = NonZeroU64::MIN;

/// Default upper bound for IDs (2^63 − 1).
pub const DEFAULT_MAX: Id = Id::MAX;

/// How a collection makes issued IDs durable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Durability {
    /// Every issued ID is on disk before it is returned.
    #[default]
    Strict,
    /// `size` IDs are reserved on disk at a time and served from memory. A crash
    /// skips the rest of the block (a gap), never reissues it.
    Block { size: NonZeroU32 },
}

/// The settings a caller chooses when creating a collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionSpec {
    pub name: String,
    pub start: Id,
    pub step: NonZeroU64,
    pub max: Id,
    pub protected: bool,
    #[serde(default)]
    pub durability: Durability,
}

impl CollectionSpec {
    /// A spec with default `start`, `step`, `max`, `protected` and `durability`.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start: DEFAULT_START,
            step: DEFAULT_STEP,
            max: DEFAULT_MAX,
            protected: false,
            durability: Durability::Strict,
        }
    }

    /// Every rule this spec breaks.
    #[must_use]
    pub fn problems(&self) -> ValidationErrors {
        let block_too_big = matches!(
            self.durability,
            Durability::Block { size } if size.get() > MAX_BLOCK_SIZE
        );
        let errors = match name_problem(&self.name) {
            Some(problem) => ValidationErrors::field("name", problem),
            None => ValidationErrors::default(),
        };
        errors
            .check(
                self.start > self.max,
                "max",
                "must be greater than or equal to start",
            )
            .check(
                block_too_big,
                "durability",
                "block size must be at most 1000000",
            )
    }
}

/// Changes to an existing collection. `None` leaves a field as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionPatch {
    pub start: Option<Id>,
    pub step: Option<NonZeroU64>,
    pub max: Option<Id>,
    pub protected: Option<bool>,
    pub durability: Option<Durability>,
}

/// A stored collection: its settings plus the counter state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    spec: CollectionSpec,
    created_at: SystemTime,
    last_id: Option<Id>,
}

/// A contiguous run of IDs issued by one `advance`: `first`, `first + step`, …, `last`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedRange {
    pub first: Id,
    pub last: Id,
    pub count: NonZeroU32,
    pub step: NonZeroU64,
}

/// The counter before and after a reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetOutcome {
    pub previous_last_id: Option<Id>,
    pub last_id: Option<Id>,
    pub next_id: Option<Id>,
}

/// Issuing the requested IDs would pass `max` or the range of [`Id`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("sequence exhausted")]
pub struct Exhausted;

impl Collection {
    /// A new, validated collection that has not issued any ID yet.
    ///
    /// # Errors
    ///
    /// Every rule the spec breaks (see [`CollectionSpec::problems`]).
    pub fn create(spec: CollectionSpec, created_at: SystemTime) -> Result<Self, ValidationErrors> {
        spec.problems().into_result(Self {
            spec,
            created_at,
            last_id: None,
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.spec.name
    }

    #[must_use]
    pub fn spec(&self) -> &CollectionSpec {
        &self.spec
    }

    #[must_use]
    pub fn created_at(&self) -> SystemTime {
        self.created_at
    }

    /// The most recently issued ID, or `None` if none has been issued.
    #[must_use]
    pub fn last_id(&self) -> Option<Id> {
        self.last_id
    }

    /// The ID the next `advance` would return first, or `None` if exhausted.
    #[must_use]
    pub fn next_id(&self) -> Option<Id> {
        let next = match self.last_id {
            None => Some(self.spec.start),
            Some(last) => last.checked_add_unsigned(self.spec.step.get()),
        };
        next.filter(|id| *id <= self.spec.max)
    }

    /// The highest ID this collection can ever issue: `max`, rounded down onto the
    /// step grid (`start`, `start + step`, …).
    #[must_use]
    pub fn highest_id(&self) -> Id {
        let span = Self::offset(self.spec.start, self.spec.max);
        let on_grid = span - span % self.spec.step.get();
        // `start + on_grid <= max`, so this cannot overflow.
        self.spec
            .start
            .checked_add_unsigned(on_grid)
            .unwrap_or(self.spec.max)
    }

    /// The last ID of a run of `count` IDs starting at `first`, if it fits under `max`.
    fn last_of_run(&self, first: Id, count: NonZeroU32) -> Option<Id> {
        let span = self
            .spec
            .step
            .get()
            .checked_mul(u64::from(count.get() - 1))?;
        first
            .checked_add_unsigned(span)
            .filter(|last| *last <= self.spec.max)
    }

    /// Issue `count` IDs: the updated collection and the issued range.
    ///
    /// # Errors
    ///
    /// [`Exhausted`] if any of them would pass `max`; then nothing is issued.
    pub fn advance(&self, count: NonZeroU32) -> Result<(Self, IssuedRange), Exhausted> {
        let first = self.next_id().ok_or(Exhausted)?;
        let last = self.last_of_run(first, count).ok_or(Exhausted)?;
        let range = IssuedRange {
            first,
            last,
            count,
            step: self.spec.step,
        };
        Ok((self.with_last_id(Some(last)), range))
    }

    /// A copy with a different counter position. For backends restoring state.
    #[must_use]
    pub fn with_last_id(&self, last_id: Option<Id>) -> Self {
        Self {
            last_id,
            ..self.clone()
        }
    }

    /// Apply a patch.
    ///
    /// # Errors
    ///
    /// Every failing field: `start`/`step` after the first ID, `max` below `last_id`,
    /// or any rule of [`CollectionSpec::problems`].
    pub fn apply(&self, patch: &CollectionPatch) -> Result<Self, ValidationErrors> {
        let issued = self.last_id.is_some();
        let spec = CollectionSpec {
            name: self.spec.name.clone(),
            start: patch.start.unwrap_or(self.spec.start),
            step: patch.step.unwrap_or(self.spec.step),
            max: patch.max.unwrap_or(self.spec.max),
            protected: patch.protected.unwrap_or(self.spec.protected),
            durability: patch.durability.unwrap_or(self.spec.durability),
        };
        let frozen = "cannot change after the first ID is issued";
        let start_changed = spec.start != self.spec.start;
        let step_changed = spec.step != self.spec.step;
        let below_last = self.last_id.is_some_and(|last| spec.max < last);
        spec.problems()
            .check(issued && start_changed, "start", frozen)
            .check(issued && step_changed, "step", frozen)
            .check(below_last, "max", "must not be below last_id")
            .into_result(Self {
                spec,
                ..self.clone()
            })
    }

    /// Move the counter so that the next ID is `value` (default: `start`).
    ///
    /// # Errors
    ///
    /// `value` outside `[start, max]` or off the step grid; or a backwards move
    /// without `allow_rewind`.
    pub fn reset(
        &self,
        value: Option<Id>,
        allow_rewind: bool,
    ) -> Result<(Self, ResetOutcome), ValidationErrors> {
        let start = self.spec.start;
        let next = value.unwrap_or(start);
        let in_range = (start..=self.spec.max).contains(&next);
        let on_grid = in_range && Self::offset(start, next).is_multiple_of(self.spec.step.get());
        let rewinds = self.last_id.is_some_and(|last| next <= last);
        ValidationErrors::default()
            .check(!in_range, "value", "must be within [start, max]")
            .check(
                in_range && !on_grid,
                "value",
                "must be start plus a multiple of step",
            )
            .check(
                rewinds && !allow_rewind,
                "allow_rewind",
                "must be true to move the counter backwards",
            )
            .into_result(())?;

        // On the grid and above `start`, so `next - step >= start` cannot overflow.
        let last_id = (next != start)
            .then(|| next.checked_sub_unsigned(self.spec.step.get()))
            .flatten();
        let updated = self.with_last_id(last_id);
        let outcome = ResetOutcome {
            previous_last_id: self.last_id,
            last_id,
            next_id: updated.next_id(),
        };
        Ok((updated, outcome))
    }

    /// `to - from` as an unsigned distance; callers ensure `from <= to`.
    fn offset(from: Id, to: Id) -> u64 {
        to.abs_diff(from)
    }
}

#[cfg(test)]
#[path = "collection_tests.rs"]
mod tests;
