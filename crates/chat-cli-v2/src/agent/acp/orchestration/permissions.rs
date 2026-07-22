//! Permission model for inter-session operations.

use std::collections::HashMap;

use sacp::schema::SessionId;

/// Tracks parent-child relationships and group membership.
#[derive(Debug, Default, Clone)]
pub struct PermissionStore {
    /// Maps session_id -> parent_session_id
    parents: HashMap<String, String>,
    /// Maps session_id -> group name
    groups: HashMap<String, String>,
}

impl PermissionStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a parent-child relationship.
    pub fn register_child(&mut self, parent: &SessionId, child: &SessionId) {
        self.parents.insert(child.to_string(), parent.to_string());
    }

    /// Register a session in a group.
    pub fn register_group(&mut self, session_id: &SessionId, group: &str) {
        self.groups.insert(session_id.to_string(), group.to_string());
    }

    /// Check if sender may interrupt or inject context into target.
    ///
    /// Rules:
    /// - No targeting self
    /// - Can target sessions you spawned (you are parent)
    /// - Can target your parent
    /// - Can target sessions in your group
    pub fn can_interact(&self, sender: &SessionId, target: &SessionId) -> Result<(), String> {
        let sender_id = sender.to_string();
        let target_id = target.to_string();

        // No targeting self
        if sender_id == target_id {
            return Err("Cannot target self".to_string());
        }

        // Check parent-child relationship
        if self.parents.get(&target_id).is_some_and(|p| p == &sender_id) {
            return Ok(());
        }
        if self.parents.get(&sender_id).is_some_and(|p| p == &target_id) {
            return Ok(());
        }

        // Check group membership
        if let (Some(sender_group), Some(target_group)) = (self.groups.get(&sender_id), self.groups.get(&target_id))
            && sender_group == target_group
        {
            return Ok(());
        }

        Err(format!(
            "Session {} is not authorized to interact with session {}",
            sender_id, target_id
        ))
    }

    /// Remove all tracking for a terminated session.
    pub fn remove_session(&mut self, session_id: &SessionId) {
        let id = session_id.to_string();
        self.parents.remove(&id);
        self.groups.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid(s: &str) -> SessionId {
        SessionId::new(s.to_string())
    }

    #[test]
    fn test_parent_child_can_interact() {
        let mut store = PermissionStore::new();
        let parent = sid("parent");
        let child = sid("child");
        store.register_child(&parent, &child);

        assert!(store.can_interact(&parent, &child).is_ok());
        assert!(store.can_interact(&child, &parent).is_ok());
    }

    #[test]
    fn test_no_self_interaction() {
        let store = PermissionStore::new();
        let session = sid("session-1");
        assert!(store.can_interact(&session, &session).is_err());
    }

    #[test]
    fn test_group_can_interact() {
        let mut store = PermissionStore::new();
        let a = sid("a");
        let b = sid("b");
        store.register_group(&a, "team");
        store.register_group(&b, "team");

        assert!(store.can_interact(&a, &b).is_ok());
    }

    #[test]
    fn test_unrelated_cannot_interact() {
        let store = PermissionStore::new();
        let a = sid("a");
        let b = sid("b");
        assert!(store.can_interact(&a, &b).is_err());
    }

    #[test]
    fn test_different_groups_cannot_interact() {
        let mut store = PermissionStore::new();
        let a = sid("a");
        let b = sid("b");
        store.register_group(&a, "team1");
        store.register_group(&b, "team2");

        assert!(store.can_interact(&a, &b).is_err());
        assert!(store.can_interact(&b, &a).is_err());
    }

    #[test]
    fn test_remove_session_clears_permissions() {
        let mut store = PermissionStore::new();
        let parent = sid("parent");
        let child = sid("child");
        store.register_child(&parent, &child);
        store.register_group(&child, "team");

        assert!(store.can_interact(&parent, &child).is_ok());

        store.remove_session(&child);
        assert!(store.can_interact(&parent, &child).is_err());
    }

    #[test]
    fn test_bidirectional_parent_child() {
        let mut store = PermissionStore::new();
        let parent = sid("parent");
        let child = sid("child");
        store.register_child(&parent, &child);

        // Both directions should work
        assert!(store.can_interact(&parent, &child).is_ok());
        assert!(store.can_interact(&child, &parent).is_ok());
    }

    #[test]
    fn test_register_multiple_groups() {
        let mut store = PermissionStore::new();
        let session = sid("session");
        let other = sid("other");

        store.register_group(&session, "team1");
        store.register_group(&other, "team1");
        assert!(store.can_interact(&session, &other).is_ok());

        // Register in different group - last wins
        store.register_group(&session, "team2");
        assert!(store.can_interact(&session, &other).is_err());

        // Verify session is now in team2
        store.register_group(&other, "team2");
        assert!(store.can_interact(&session, &other).is_ok());
    }
}
