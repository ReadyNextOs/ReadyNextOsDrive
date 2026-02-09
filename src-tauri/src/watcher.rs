use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// File system watcher for detecting local changes.
///
/// When a file changes locally, triggers an immediate sync
/// instead of waiting for the next scheduled sync interval.
pub struct FileWatcher {
    watcher: Option<RecommendedWatcher>,
    rx: Option<mpsc::Receiver<Result<Event, notify::Error>>>,
}

impl FileWatcher {
    pub fn new() -> Self {
        Self {
            watcher: None,
            rx: None,
        }
    }

    /// Start watching the given directories for changes.
    pub fn start(&mut self, paths: &[&Path]) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default()
                .with_poll_interval(Duration::from_secs(2)),
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

    /// Check if there are pending file change events.
    /// Returns true if changes were detected.
    pub fn has_changes(&self) -> bool {
        if let Some(rx) = &self.rx {
            // Non-blocking check for events
            match rx.try_recv() {
                Ok(Ok(event)) => {
                    log::debug!("File change detected: {:?}", event.kind);
                    // Drain any remaining events
                    while rx.try_recv().is_ok() {}
                    true
                }
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
