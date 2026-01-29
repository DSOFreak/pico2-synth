#![allow(unused_macros, unused_imports)]
macro_rules! arr {
    ($producer:expr) => {
        core::array::from_fn($producer)
    };
    ($producer:expr; $N:literal) => {
        core::array::from_fn::<_,$N,_>($producer)
    };
    ($($val:expr),+) => {
        [$($val),+]
    }
}
pub(crate) use arr;

