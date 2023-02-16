use std::{fmt::write, os::windows::process};
use std::process::exit;

#[derive(Copy, Clone, Debug)]
pub enum LogType {
    Error,
    Warning,
    Info,
    Debug,
}

impl std::fmt::Display for LogType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let type_str = match *self {
            LogType::Error => "Error",
            LogType::Warning => "Warning",
            LogType::Info => "Info",
            LogType::Debug => "Debug"
        };

        write!(f, "{}", type_str)
    }
}


pub fn out(log_type: LogType, msg: &str) {
    let full_msg = format!("{:<10}: {} ", log_type, msg);

    match log_type {
        LogType::Error => {
            eprintln!("{}", full_msg);
        },
        _     => {
            println!("{}", full_msg)
        } ,
    }
}

macro_rules! error {
    ($($arg:tt)*) => {{
        tlog::out(tlog::LogType::Error, &format!($($arg)*));
    }};
}

macro_rules! warning {
    ($($arg:tt)*) => {{
        tlog::out(tlog::LogType::Warning, &format!($($arg)*));
    }};
}

macro_rules! info {
    ($($arg:tt)*) => {{
        tlog::out(tlog::LogType::Info, &format!($($arg)*));
    }};
}

macro_rules! debug {
    ($($arg:tt)*) => {{
        tlog::out(tlog::LogType::Debug, &format!($($arg)*));
    }};
}

pub(crate) use error;
pub(crate) use warning;
pub(crate) use info;
pub(crate) use debug;
