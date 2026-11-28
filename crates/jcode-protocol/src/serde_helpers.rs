//! Serde predicates used by the wire types.
//!
//! These live outside `wire.rs` so that file stays inside its code-size
//! ratchet; the wire declarations themselves remain in one place.

/// Default for boolean fields that mean `true` when absent, so older clients
/// that omit the field keep their previous (unconditional) behavior.
pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}
