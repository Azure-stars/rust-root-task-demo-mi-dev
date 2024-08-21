use core::fmt;

use log::{Level, LevelFilter, Log, Metadata, Record};

pub struct Logger;

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let color_code = match record.level() {
            Level::Error => 31u8, // Red
            Level::Warn => 93,    // BrightYellow
            Level::Info => 34,    // Blue
            Level::Debug => 32,   // Green
            Level::Trace => 90,   // BrightBlack
        };
        #[cfg(feature = "with_line")]
        {
            let file = record.file();
            let line = record.line();
            sel4::debug_println!(
                "\u{1B}[{}m\
                [{}] {}:{} {}\
                \u{1B}[0m\n",
                color_code,
                record.level(),
                file.unwrap(),
                line.unwrap(),
                record.args()
            )
            .expect("can't write color string in logging module.");
        }

        #[cfg(not(feature = "with_line"))]
        sel4::debug_println!(
            "\u{1B}[{}m\
            [{}] {}\
            \u{1B}[0m\n",
            color_code,
            record.level(),
            record.args()
        )
        .expect("can't write color string in logging module.");
    }

    fn flush(&self) {}
}

pub fn init(level: Option<&str>) {
    log::set_logger(&Logger).unwrap();
    log::set_max_level(match level {
        Some("error") => LevelFilter::Error,
        Some("warn") => LevelFilter::Warn,
        Some("info") => LevelFilter::Info,
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        _ => LevelFilter::Off,
    });
    info!("logging module initialized");
}

#[inline]
pub fn print_args(args: fmt::Arguments) {
    Logger
        .write_fmt(args)
        .expect("can't write string in logging module.");
}

#[inline]
pub fn put_char(c: u8) {
    sel4::debug_put_char(c)
}

#[inline]
pub fn get_char() -> u8 {
    todo!("get character")
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        $crate::logging::print_args(format_args!($($arg)*));
    });
}

#[macro_export]
macro_rules! println {
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
}
