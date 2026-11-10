//! Core of idbseq: the collection model and validation, the sequence engine and
//! the [`store::SequenceStore`] storage trait with its backends.

pub mod collection;
pub mod engine;
pub mod store;
pub mod validation;
