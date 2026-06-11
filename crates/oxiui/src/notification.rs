//! In-app toast notification queue.

/// A pending in-app toast notification.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Short title line.
    pub title: String,
    /// Longer body text.
    pub body: String,
    /// How long the notification should be displayed, in milliseconds.
    pub duration_ms: u64,
    /// Urgency level: 0 = low, 1 = normal, 2 = critical.
    pub urgency: u8,
    /// When the notification was created.
    pub created_at: std::time::Instant,
}

/// A FIFO queue of pending [`Notification`]s.
///
/// Call [`NotificationQueue::push`] to enqueue notifications, and
/// [`NotificationQueue::pop_due`] each frame to drain them for display.
pub struct NotificationQueue {
    pending: std::collections::VecDeque<Notification>,
}

impl NotificationQueue {
    /// Create an empty [`NotificationQueue`].
    pub fn new() -> Self {
        Self {
            pending: std::collections::VecDeque::new(),
        }
    }

    /// Enqueue a notification.
    pub fn push(&mut self, title: impl Into<String>, body: impl Into<String>, duration_ms: u64) {
        self.pending.push_back(Notification {
            title: title.into(),
            body: body.into(),
            duration_ms,
            urgency: 1,
            created_at: std::time::Instant::now(),
        });
    }

    /// Enqueue a notification with explicit urgency (0=low, 1=normal, 2=critical).
    pub fn enqueue(&mut self, title: impl Into<String>, body: impl Into<String>, urgency: u8) {
        let duration_ms = match urgency {
            0 => 3_000,
            2 => 10_000,
            _ => 5_000,
        };
        self.pending.push_back(Notification {
            title: title.into(),
            body: body.into(),
            duration_ms,
            urgency,
            created_at: std::time::Instant::now(),
        });
    }

    /// Dequeue the next pending notification, if any.
    pub fn pop_due(&mut self) -> Option<Notification> {
        self.pending.pop_front()
    }

    /// Returns `true` if no notifications are pending.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// The number of pending notifications.
    pub fn len(&self) -> usize {
        self.pending.len()
    }
}

impl Default for NotificationQueue {
    fn default() -> Self {
        Self::new()
    }
}
