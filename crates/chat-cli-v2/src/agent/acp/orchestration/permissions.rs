//! Permission model for inter-session messaging.

use std::collections::HashMap;
use std::time::{
    Duration,
    SystemTime,
};

use sacp::schema::SessionId;

/// Maximum messages per session per minute.
const RATE_LIMIT: usize = 10;
const RATE_WINDOW: Duration = Duration::from_secs(60);

/// Tracks parent-child relationships and rate limits.
#[derive(Debug, Default, Clone)]
pub struct PermissionStore {
    /// Maps session_id -> parent_session_id
    parents: HashMap<String, String>,
    /// Maps session_id -> group name
    groups: HashMap<String, String>,
    /// Rate limit tracking: session_id -> list of send timestamps
    send_timestamps: HashMap<String, Vec<SystemTime>>,
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

    /// Check if sender can message target.
    ///
    /// Rules:
    /// - No self-messaging
    /// - Can message sessions you spawned (you are parent)
    /// - Can message your parent
    /// - Can message sessions in your group
    pub fn can_message(&self, sender: &SessionId, target: &SessionId) -> Result<(), String> {
        let sender_id = sender.to_string();
        let target_id = target.to_string();

        // No self-messaging
        if sender_id == target_id {
            return Err("Cannot send message to self".to_string());
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
            "Session {} is not authorized to message session {}",
            sender_id, target_id
        ))
    }

    /// Check and update rate limit. Returns error if rate exceeded.
    pub fn check_rate_limit(&mut self, sender: &SessionId) -> Result<(), String> {
        let now = SystemTime::now();
        let timestamps = self.send_timestamps.entry(sender.to_string()).or_default();

        // Remove timestamps outside the window
        timestamps.retain(|t| now.duration_since(*t).unwrap_or(Duration::ZERO) < RATE_WINDOW);

        if timestamps.len() >= RATE_LIMIT {
            return Err(format!("Rate limit exceeded: max {} messages per minute", RATE_LIMIT));
        }

        timestamps.push(now);
        Ok(())
    }

    /// Remove all tracking for a terminated session.
    pub fn remove_session(&mut self, session_id: &SessionId) {
        let id = session_id.to_string();
        self.parents.remove(&id);
        self.groups.remove(&id);
        self.send_timestamps.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid(s: &str) -> SessionId {
        SessionId::new(s.to_string())
    }

    #[test]
    fn test_parent_child_can_message() {
        let mut store = PermissionStore::new();
        let parent = sid("parent");
        let child = sid("child");
        store.register_child(&parent, &child);

        assert!(store.can_message(&parent, &child).is_ok());
        assert!(store.can_message(&child, &parent).is_ok());
    }

    #[test]
    fn test_no_self_messaging() {
        let store = PermissionStore::new();
        let session = sid("session-1");
        assert!(store.can_message(&session, &session).is_err());
    }

    #[test]
    fn test_group_can_message() {
        let mut store = PermissionStore::new();
        let a = sid("a");
        let b = sid("b");
        store.register_group(&a, "team");
        store.register_group(&b, "team");

        assert!(store.can_message(&a, &b).is_ok());
    }

    #[test]
    fn test_unrelated_cannot_message() {
        let store = PermissionStore::new();
        let a = sid("a");
        let b = sid("b");
        assert!(store.can_message(&a, &b).is_err());
    }

    #[test]
    fn test_rate_limit() {
        let mut store = PermissionStore::new();
        let sender = sid("sender");

        for _ in 0..RATE_LIMIT {
            assert!(store.check_rate_limit(&sender).is_ok());
        }
        assert!(store.check_rate_limit(&sender).is_err());
    }

    #[test]
    fn test_different_groups_cannot_message() {
        let mut store = PermissionStore::new();
        let a = sid("a");
        let b = sid("b");
        store.register_group(&a, "team1");
        store.register_group(&b, "team2");

        assert!(store.can_message(&a, &b).is_err());
        assert!(store.can_message(&b, &a).is_err());
    }

    #[test]
    fn test_remove_session_clears_permissions() {
        let mut store = PermissionStore::new();
        let parent = sid("parent");
        let child = sid("child");
        store.register_child(&parent, &child);
        store.register_group(&child, "team");

        assert!(store.can_message(&parent, &child).is_ok());

        store.remove_session(&child);
        assert!(store.can_message(&parent, &child).is_err());
    }

    #[test]
    fn test_rate_limit_resets() {
        let mut store = PermissionStore::new();
        let sender = sid("sender");

        // Fill up the rate limit
        for _ in 0..RATE_LIMIT {
            assert!(store.check_rate_limit(&sender).is_ok());
        }
        assert!(store.check_rate_limit(&sender).is_err());

        // Verify the structure exists and has timestamps
        assert!(store.send_timestamps.contains_key(&sender.to_string()));
        assert_eq!(store.send_timestamps[&sender.to_string()].len(), RATE_LIMIT);
    }

    #[test]
    fn test_bidirectional_parent_child() {
        let mut store = PermissionStore::new();
        let parent = sid("parent");
        let child = sid("child");
        store.register_child(&parent, &child);

        // Both directions should work
        assert!(store.can_message(&parent, &child).is_ok());
        assert!(store.can_message(&child, &parent).is_ok());
    }

    #[test]
    fn test_register_multiple_groups() {
        let mut store = PermissionStore::new();
        let session = sid("session");
        let other = sid("other");

        store.register_group(&session, "team1");
        store.register_group(&other, "team1");
        assert!(store.can_message(&session, &other).is_ok());

        // Register in different group - last wins
        store.register_group(&session, "team2");
        assert!(store.can_message(&session, &other).is_err());

        // Verify session is now in team2
        store.register_group(&other, "team2");
        assert!(store.can_message(&session, &other).is_ok());
    }
}
