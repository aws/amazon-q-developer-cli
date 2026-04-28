//! Inbox storage and message routing for orchestrated sessions.

use std::collections::HashMap;
use std::time::SystemTime;

use sacp::schema::SessionId;
use tracing::{
    info,
    warn,
};

use super::types::InboxMessage;

/// Maximum messages per inbox before oldest are dropped.
const MAX_INBOX_SIZE: usize = 100;

/// Maximum message size in characters (~4K tokens).
const MAX_MESSAGE_SIZE: usize = 16_000;

/// Per-session inbox storage.
#[derive(Debug, Default, Clone)]
pub struct InboxStore {
    inboxes: HashMap<String, Vec<InboxMessage>>,
    /// Track recent sends: (sender, target) -> timestamps
    recent_sends: HashMap<(String, String), Vec<SystemTime>>,
}

/// Summary of unread messages for system prompt injection.
#[derive(Debug, Clone)]
pub struct InboxSummary {
    pub unread_count: usize,
    pub escalation_count: usize,
    /// Top senders with their unread counts (max 5).
    pub senders: Vec<(String, usize)>,
}

impl InboxStore {
    pub fn new() -> Self {
        Self {
            inboxes: HashMap::new(),
            recent_sends: HashMap::new(),
        }
    }

    /// Store a message in the target session's inbox.
    ///
    /// Returns an error string if the message is too large or the inbox is full.
    pub fn send_message(
        &mut self,
        target_session: &SessionId,
        from_session: &SessionId,
        from_name: &str,
        message: String,
        is_escalation: bool,
    ) -> Result<(), String> {
        if message.len() > MAX_MESSAGE_SIZE {
            return Err(format!(
                "Message too large: {} chars (max {})",
                message.len(),
                MAX_MESSAGE_SIZE
            ));
        }

        // Check for circular messaging (same sender → same target, rapid fire)
        let key = (from_session.to_string(), target_session.to_string());
        let now = SystemTime::now();
        let cutoff = now.checked_sub(std::time::Duration::from_secs(5)).unwrap_or(now);

        let recent = self.recent_sends.entry(key).or_default();
        recent.retain(|&timestamp| timestamp > cutoff);

        if recent.len() >= 200 {
            return Err("Circular messaging detected: too many messages to same target in 5 seconds".to_string());
        }

        recent.push(now);

        let inbox = self.inboxes.entry(target_session.to_string()).or_default();

        // Drop oldest if at capacity
        if inbox.len() >= MAX_INBOX_SIZE {
            let removed = inbox.remove(0);
            warn!(
                target = %target_session.to_string(),
                from = %removed.from_name,
                "Inbox full, dropped oldest message"
            );
        }

        let msg = InboxMessage {
            from_session: from_session.clone(),
            from_name: from_name.to_string(),
            message,
            timestamp: SystemTime::now(),
            read: false,
            is_escalation,
        };

        info!(
            target = %target_session.to_string(),
            from = %from_name,
            "Message delivered to inbox"
        );

        inbox.push(msg);
        Ok(())
    }

    /// Read messages from a session's inbox, marking them as read.
    ///
    /// Returns up to `limit` unread messages, oldest first.
    /// Peek at messages from specific senders without marking as read.
    pub fn peek_from(&self, session_id: &SessionId, senders: &[&str]) -> Vec<InboxMessage> {
        let inbox = match self.inboxes.get(&session_id.to_string()) {
            Some(inbox) => inbox,
            None => return vec![],
        };
        inbox
            .iter()
            .filter(|m| senders.iter().any(|s| m.from_name == *s))
            .cloned()
            .collect()
    }

    pub fn read_messages(&mut self, session_id: &SessionId, limit: usize) -> Vec<InboxMessage> {
        let inbox = match self.inboxes.get_mut(&session_id.to_string()) {
            Some(inbox) => inbox,
            None => return vec![],
        };

        let mut result = Vec::new();
        for msg in inbox.iter_mut() {
            if !msg.read && result.len() < limit {
                msg.read = true;
                result.push(msg.clone());
            }
        }

        info!(
            session = %session_id.to_string(),
            count = result.len(),
            "Messages read from inbox"
        );

        result
    }

    /// Get a summary of unread messages for system prompt injection.
    pub fn get_unread_summary(&self, session_id: &SessionId) -> InboxSummary {
        let inbox = match self.inboxes.get(&session_id.to_string()) {
            Some(inbox) => inbox,
            None => {
                return InboxSummary {
                    unread_count: 0,
                    escalation_count: 0,
                    senders: vec![],
                };
            },
        };

        let mut sender_counts: HashMap<String, usize> = HashMap::new();
        let mut unread_count = 0;
        let mut escalation_count = 0;

        for msg in inbox.iter() {
            if !msg.read {
                unread_count += 1;
                if msg.is_escalation {
                    escalation_count += 1;
                }
                *sender_counts.entry(msg.from_name.clone()).or_default() += 1;
            }
        }

        // Sort by count descending, take top 5
        let mut senders: Vec<(String, usize)> = sender_counts.into_iter().collect();
        senders.sort_by(|a, b| b.1.cmp(&a.1));
        senders.truncate(5);

        InboxSummary {
            unread_count,
            escalation_count,
            senders,
        }
    }

    /// Format the inbox summary as a system prompt section.
    pub fn format_inbox_prompt(&self, session_id: &SessionId) -> Option<String> {
        let summary = self.get_unread_summary(session_id);
        if summary.unread_count == 0 {
            return None;
        }

        let mut prompt = String::new();

        // Show escalations first if any
        if summary.escalation_count > 0 {
            prompt.push_str(&format!("\n## ⚠️ Escalations ({} urgent)\n", summary.escalation_count));
            // List escalation messages
            if let Some(inbox) = self.inboxes.get(&session_id.to_string()) {
                for msg in inbox.iter().filter(|m| !m.read && m.is_escalation) {
                    prompt.push_str(&format!(
                        "  - ⚠️ {} [ESCALATION]: {}\n",
                        msg.from_name,
                        truncate_msg(&msg.message, 120)
                    ));
                }
            }
            prompt.push_str("Handle escalations IMMEDIATELY before other work.\n");
        }

        let normal_count = summary.unread_count - summary.escalation_count;
        if normal_count > 0 {
            prompt.push_str(&format!("\n## Messages ({} unread)\n", normal_count));
            for (name, count) in &summary.senders {
                prompt.push_str(&format!(
                    "  - {}: {} message{}\n",
                    name,
                    count,
                    if *count > 1 { "s" } else { "" }
                ));
            }
            prompt.push_str("Use read_messages to retrieve full content when ready.\n");
        }

        Some(prompt)
    }

    /// Check if a session has unread messages.
    pub fn has_unread(&self, session_id: &SessionId) -> bool {
        self.inboxes
            .get(&session_id.to_string())
            .is_some_and(|inbox| inbox.iter().any(|m| !m.read))
    }

    /// Remove all messages for a terminated session.
    pub fn remove_inbox(&mut self, session_id: &SessionId) {
        self.inboxes.remove(&session_id.to_string());
    }
}

fn truncate_msg(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        agent::util::truncate_safe(s, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session_id(s: &str) -> SessionId {
        SessionId::new(s.to_string())
    }

    #[test]
    fn test_send_and_read_messages() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        store
            .send_message(&target, &sender, "Reader", "Hello from Reader".to_string(), false)
            .unwrap();
        store
            .send_message(&target, &sender, "Reader", "Second message".to_string(), false)
            .unwrap();

        let summary = store.get_unread_summary(&target);
        assert_eq!(summary.unread_count, 2);
        assert_eq!(summary.senders.len(), 1);
        assert_eq!(summary.senders[0].0, "Reader");

        let messages = store.read_messages(&target, 5);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].message, "Hello from Reader");

        // After reading, unread count should be 0
        let summary = store.get_unread_summary(&target);
        assert_eq!(summary.unread_count, 0);
    }

    #[test]
    fn test_inbox_overflow() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        for i in 0..101 {
            store
                .send_message(&target, &sender, "Sender", format!("Message {}", i), false)
                .unwrap();
        }

        // Should have MAX_INBOX_SIZE messages, oldest dropped
        let messages = store.read_messages(&target, 200);
        assert_eq!(messages.len(), MAX_INBOX_SIZE);
        assert_eq!(messages[0].message, "Message 1"); // Message 0 was dropped
    }

    #[test]
    fn test_message_too_large() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        let large_msg = "x".repeat(MAX_MESSAGE_SIZE + 1);
        let result = store.send_message(&target, &sender, "Sender", large_msg, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_inbox_prompt() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender1 = make_session_id("session-2");
        let sender2 = make_session_id("session-3");

        store
            .send_message(&target, &sender1, "Reader", "msg1".to_string(), false)
            .unwrap();
        store
            .send_message(&target, &sender2, "Neagley", "msg2".to_string(), false)
            .unwrap();
        store
            .send_message(&target, &sender1, "Reader", "msg3".to_string(), false)
            .unwrap();

        let prompt = store.format_inbox_prompt(&target).unwrap();
        assert!(prompt.contains("3 unread"));
        assert!(prompt.contains("Reader: 2 messages"));
        assert!(prompt.contains("Neagley: 1 message"));
    }

    #[test]
    fn test_no_unread_returns_none() {
        let store = InboxStore::new();
        let target = make_session_id("session-1");
        assert!(store.format_inbox_prompt(&target).is_none());
    }

    #[test]
    fn test_read_with_limit() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        for i in 0..10 {
            store
                .send_message(&target, &sender, "Sender", format!("Message {}", i), false)
                .unwrap();
        }

        let messages = store.read_messages(&target, 3);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].message, "Message 0");
        assert_eq!(messages[1].message, "Message 1");
        assert_eq!(messages[2].message, "Message 2");
    }

    #[test]
    fn test_read_marks_as_read() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        store
            .send_message(&target, &sender, "Sender", "Message 1".to_string(), false)
            .unwrap();
        store
            .send_message(&target, &sender, "Sender", "Message 2".to_string(), false)
            .unwrap();

        let messages = store.read_messages(&target, 5);
        assert_eq!(messages.len(), 2);

        let summary = store.get_unread_summary(&target);
        assert_eq!(summary.unread_count, 0);
    }

    #[test]
    fn test_multiple_senders_summary() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender1 = make_session_id("session-2");
        let sender2 = make_session_id("session-3");
        let sender3 = make_session_id("session-4");

        // Sender1: 3 messages
        for _ in 0..3 {
            store
                .send_message(&target, &sender1, "Alice", "msg".to_string(), false)
                .unwrap();
        }
        // Sender2: 1 message
        store
            .send_message(&target, &sender2, "Bob", "msg".to_string(), false)
            .unwrap();
        // Sender3: 2 messages
        for _ in 0..2 {
            store
                .send_message(&target, &sender3, "Charlie", "msg".to_string(), false)
                .unwrap();
        }

        let summary = store.get_unread_summary(&target);
        assert_eq!(summary.unread_count, 6);
        assert_eq!(summary.senders.len(), 3);
        // Should be ordered by count descending
        assert_eq!(summary.senders[0], ("Alice".to_string(), 3));
        assert_eq!(summary.senders[1], ("Charlie".to_string(), 2));
        assert_eq!(summary.senders[2], ("Bob".to_string(), 1));
    }

    #[test]
    fn test_has_unread_empty_inbox() {
        let store = InboxStore::new();
        let target = make_session_id("session-1");
        assert!(!store.has_unread(&target));
    }

    #[test]
    fn test_has_unread_after_read() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        store
            .send_message(&target, &sender, "Sender", "Message".to_string(), false)
            .unwrap();
        assert!(store.has_unread(&target));

        store.read_messages(&target, 5);
        assert!(!store.has_unread(&target));
    }

    #[test]
    fn test_remove_inbox() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        store
            .send_message(&target, &sender, "Sender", "Message".to_string(), false)
            .unwrap();
        assert!(store.has_unread(&target));

        store.remove_inbox(&target);
        assert!(!store.has_unread(&target));
        let messages = store.read_messages(&target, 5);
        assert_eq!(messages.len(), 0);
    }

    #[test]
    fn test_send_to_multiple_targets() {
        let mut store = InboxStore::new();
        let target1 = make_session_id("session-1");
        let target2 = make_session_id("session-2");
        let sender = make_session_id("session-3");

        store
            .send_message(&target1, &sender, "Sender", "Message to 1".to_string(), false)
            .unwrap();
        store
            .send_message(&target2, &sender, "Sender", "Message to 2".to_string(), false)
            .unwrap();

        let messages1 = store.read_messages(&target1, 5);
        let messages2 = store.read_messages(&target2, 5);

        assert_eq!(messages1.len(), 1);
        assert_eq!(messages2.len(), 1);
        assert_eq!(messages1[0].message, "Message to 1");
        assert_eq!(messages2[0].message, "Message to 2");
    }

    #[test]
    fn test_format_prompt_truncates_senders() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");

        // Send from 7 different senders
        for i in 0..7 {
            let sender = make_session_id(&format!("sender-{}", i));
            store
                .send_message(&target, &sender, &format!("Sender{}", i), "msg".to_string(), false)
                .unwrap();
        }

        let summary = store.get_unread_summary(&target);
        assert_eq!(summary.unread_count, 7);
        assert_eq!(summary.senders.len(), 5); // Should be truncated to 5

        let prompt = store.format_inbox_prompt(&target).unwrap();
        assert!(prompt.contains("7 unread"));
        // Count lines that contain sender info (lines with ":")
        let sender_lines = prompt
            .lines()
            .filter(|line| line.contains(":") && line.contains("message"))
            .count();
        assert_eq!(sender_lines, 5);
    }

    #[test]
    fn test_send_escalation_message() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        store
            .send_message(&target, &sender, "Reader", "Urgent issue".to_string(), true)
            .unwrap();

        let messages = store.read_messages(&target, 5);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is_escalation);
    }

    #[test]
    fn test_escalation_count_in_summary() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        // Send 3 normal messages
        for i in 0..3 {
            store
                .send_message(&target, &sender, "Sender", format!("Normal {}", i), false)
                .unwrap();
        }
        // Send 2 escalation messages
        for i in 0..2 {
            store
                .send_message(&target, &sender, "Sender", format!("Escalation {}", i), true)
                .unwrap();
        }

        let summary = store.get_unread_summary(&target);
        assert_eq!(summary.unread_count, 5);
        assert_eq!(summary.escalation_count, 2);
    }

    #[test]
    fn test_format_inbox_prompt_with_escalations() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        store
            .send_message(&target, &sender, "Sender", "Urgent issue".to_string(), true)
            .unwrap();
        store
            .send_message(&target, &sender, "Sender", "Normal message".to_string(), false)
            .unwrap();

        let prompt = store.format_inbox_prompt(&target).unwrap();
        assert!(prompt.contains("⚠️ Escalations"));
        assert!(prompt.contains("## Messages"));
    }

    #[test]
    fn test_format_inbox_prompt_escalation_only() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        store
            .send_message(&target, &sender, "Sender", "Urgent issue".to_string(), true)
            .unwrap();

        let prompt = store.format_inbox_prompt(&target).unwrap();
        assert!(prompt.contains("⚠️ Escalations"));
        assert!(!prompt.contains("## Messages"));
    }

    #[test]
    fn test_escalation_default_false() {
        let mut store = InboxStore::new();
        let target = make_session_id("session-1");
        let sender = make_session_id("session-2");

        store
            .send_message(&target, &sender, "Sender", "Normal message".to_string(), false)
            .unwrap();

        let summary = store.get_unread_summary(&target);
        assert_eq!(summary.unread_count, 1);
        assert_eq!(summary.escalation_count, 0);
    }
}
