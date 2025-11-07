use std::{
    cmp::Ordering,
    convert::Infallible,
    num::{Saturating, Wrapping},
    path::PathBuf,
    sync::Arc,
};

pub trait Data: Clone + Send + Sync + 'static {}

macro_rules! impl_data {
    ($($t:ty),*) => {
        $(
            impl Data for $t {}
        )*
    };
}

macro_rules! impl_generic_data {
    ($($t:ident<$d:ident: $bound:tt>),*) => {
        $(
            impl<$d: $bound> Data for $t<$d> {}
        )*
    };
}

impl_data!(
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
    &'static str,
    bool,
    char,
    (),
    String,
    PathBuf,
    Ordering,
    Infallible,
    std::time::Duration,
    std::time::Instant,
    std::time::SystemTime
);

impl_generic_data!(
    Option<T: Data>,
    Box<T: Data>,
    Vec<T: Data>,
    Arc<T: Data>,
    Saturating<T: Data>,
    Wrapping<T: Data>
);

impl<const N: usize, T: Data> Data for [T; N] {}
