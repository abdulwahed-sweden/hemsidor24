//! A minimal logger for the back office.
//!
//! `rustio-admin` logs through the `log` facade, which does nothing at all
//! until something installs a logger. Without this the panel runs silent —
//! including the lines it writes when a request fails — so this exists to make
//! the framework's own diagnostics visible.
//!
//! Deliberately tiny: the only dependency it adds is the `log` facade itself,
//! which is already in the tree via rustio-admin. Pulling in a full logging
//! stack to read a handful of framework messages would be a poor trade.

use std::io::Write;

use log::{Level, LevelFilter, Metadata, Record};

/// Writes `LEVEL target: message` to stderr.
struct StderrLogger {
    level: Level,
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // Ignore write failures: a logger that panics because stderr is closed
        // is worse than one that goes quiet.
        let _ = writeln!(
            std::io::stderr(),
            "{:<5} {}: {}",
            record.level(),
            record.target(),
            record.args()
        );
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

/// Install the logger. `ADMIN_LOG` sets the level, `info` by default.
///
/// Safe to call more than once; the second call is ignored.
pub fn init() {
    let level = match std::env::var("ADMIN_LOG").as_deref().map(str::trim) {
        Ok("error") => Level::Error,
        Ok("warn") => Level::Warn,
        Ok("debug") => Level::Debug,
        Ok("trace") => Level::Trace,
        _ => Level::Info,
    };

    let logger = Box::leak(Box::new(StderrLogger { level }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(LevelFilter::Trace);
    }
}
