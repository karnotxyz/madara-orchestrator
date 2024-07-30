pub mod collections;
pub mod conversions;
pub mod env_utils;
pub mod settings;
pub mod time;

/// Evaluate `$x:expr` and if not true return `Err($y:expr)`.
///
/// Used as `ensure!(expression_to_ensure, expression_to_return_on_false)`.
#[macro_export]
macro_rules! ensure {
    ($x:expr, $y:expr $(,)?) => {{
        if !$x {
            return Err($y);
        }
    }};
}
