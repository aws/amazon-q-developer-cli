use crate::ManagedValue;

/// Linux has no OS-native managed configuration surface.
/// Returns `None` unconditionally.
pub(crate) fn read_managed_string(_key: &str) -> Option<ManagedValue> {
    None
}
