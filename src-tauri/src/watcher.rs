use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

/// File system watcher for detecting local changes.
///
/// When a file changes locally, sends a notification via an async channel
/// so the scheduler can react immediately (Synology Drive-style).
pub struct FileWatcher {
    watcher: Option<RecommendedWatcher>,
    /// Async receiver — scheduler awaits this for instant reaction.
    rx: Option<mpsc::Receiver<()>>,
}

impl FileWatcher {
    pub fn new() -> Self {
        Self {
            watcher: None,
            rx: None,
        }
    }

    /// Start watching the given directories for changes.
    /// Returns a channel receiver that fires whenever a relevant file changes.
    pub fn start(&mut self, paths: &[&Path]) -> Result<(), String> {
        // Bounded channel — if sync can't keep up, events coalesce (capacity 1).
        let (tx, rx) = mpsc::channel::<()>(1);

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    // Only trigger on real content changes, not metadata-only.
                    if matches!(
                        event.kind,
                        EventKind::Create(_)
                            | EventKind::Modify(notify::event::ModifyKind::Data(_))
                            | EventKind::Modify(notify::event::ModifyKind::Name(_))
                            | EventKind::Remove(_)
                    ) {
                        // try_send: if channel is full (sync already pending), skip.
                        let _ = tx.try_send(());
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

        for path in paths {
            if path.exists() {
                watcher
                    .watch(path, RecursiveMode::Recursive)
                    .map_err(|e| format!("Failed to watch {:?}: {}", path, e))?;
                log::info!("Watching directory: {:?}", path);
            }
        }

        self.watcher = Some(watcher);
        self.rx = Some(rx);

        Ok(())
    }

    /// Stop watching.
    pub fn stop(&mut self) {
        self.watcher = None;
        self.rx = None;
    }

    pub fn is_running(&self) -> bool {
        self.watcher.is_some()
    }

    /// Take the async receiver. The scheduler loop owns it and awaits on it
    /// for instant reaction to file changes.
    pub fn take_receiver(&mut self) -> Option<mpsc::Receiver<()>> {
        self.rx.take()
    }

    /// Check if there are pending file change events (non-blocking, legacy).
    pub fn has_changes(&mut self) -> bool {
        if let Some(rx) = &mut self.rx {
            match rx.try_recv() {
                Ok(()) => true,
                _ => false,
            }
        } else {
            false
        }
    }
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new()
    }
}
