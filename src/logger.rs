use std::sync::atomic::{AtomicU8, Ordering};

// Log level definitions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLevel {
    Error = 1, // Always show errors
    Warn = 2,  // Warnings and above
    Info = 3,  // General information and above
    Debug = 4, // Debug information and above
    Trace = 5, // Verbose tracing (lowest level, disabled by default)
}

impl LogLevel {
    pub fn from_str(level: &str) -> Option<Self> {
        match level.to_lowercase().as_str() {
            "error" => Some(LogLevel::Error),
            "warn" => Some(LogLevel::Warn),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            "trace" => Some(LogLevel::Trace),
            _ => None,
        }
    }
}

// Global log level configuration
static LOG_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);

pub fn set_log_level(level: LogLevel) {
    LOG_LEVEL.store(level as u8, Ordering::Relaxed);
}

pub fn get_log_level() -> LogLevel {
    let level_value = LOG_LEVEL.load(Ordering::Relaxed);
    match level_value {
        1 => LogLevel::Error,
        2 => LogLevel::Warn,
        3 => LogLevel::Info,
        4 => LogLevel::Debug,
        5 => LogLevel::Trace,
        _ => LogLevel::Info, // Default fallback
    }
}

// Logging macros
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        if $crate::logger::get_log_level() as u8 >= $crate::logger::LogLevel::Error as u8 {
            println!("❌ {}", format_args!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        if $crate::logger::get_log_level() as u8 >= $crate::logger::LogLevel::Warn as u8 {
            println!("⚠️  {}", format_args!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        if $crate::logger::get_log_level() as u8 >= $crate::logger::LogLevel::Info as u8 {
            println!("ℹ️  {}", format_args!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        if $crate::logger::get_log_level() as u8 >= $crate::logger::LogLevel::Debug as u8 {
            println!("🐛 {}", format_args!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        if $crate::logger::get_log_level() as u8 >= $crate::logger::LogLevel::Trace as u8 {
            println!("🔍 {}", format_args!($($arg)*));
        }
    };
}
