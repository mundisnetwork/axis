//! Mundis Rust-based BPF program logging

/// Print a message to the log
///
/// Fast form:
/// 1. Single string: `msg!("hi")`
///
/// The generic form incurs a very large runtime overhead so it should be used with care:
/// 3. Generalized format string: `msg!("Hello {}: 1, 2, {}", "World", 3)`
///
#[macro_export]
macro_rules! msg {
    ($msg:expr) => {
        $crate::log::sol_log($msg)
    };
    ($($arg:tt)*) => ($crate::log::sol_log(&format!($($arg)*)));
}

/// Print a string to the log
///
/// @param message - Message to print
#[inline]
pub fn sol_log(message: &str) {
    println!("{}", message);
}

/// Print 64-bit values represented as hexadecimal to the log
///
/// @param argx - integer arguments to print

#[inline]
pub fn sol_log_64(arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) {
    println!("{}", &format!(
        "{:#x}, {:#x}, {:#x}, {:#x}, {:#x}",
        arg1, arg2, arg3, arg4, arg5
    ));
}


