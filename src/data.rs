use std::{
    cmp::Ordering,
    convert::Infallible,
    num::{Saturating, Wrapping},
    path::PathBuf,
    sync::Arc,
};

pub trait Data: PartialEq + Clone + Send + Sync + 'static {}

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

macro_rules! impl_tuple_data {
    ($($t:ident),*) => {
        impl<$($t: Data,)*> Data for ($($t,)*) {}
    }
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

impl_tuple_data!(A);
impl_tuple_data!(A, B);
impl_tuple_data!(A, B, C);
impl_tuple_data!(A, B, C, D);
impl_tuple_data!(A, B, C, D, E);
impl_tuple_data!(A, B, C, D, E, F);
impl_tuple_data!(A, B, C, D, E, F, G);
impl_tuple_data!(A, B, C, D, E, F, G, H);
impl_tuple_data!(A, B, C, D, E, F, G, H, I);
impl_tuple_data!(A, B, C, D, E, F, G, H, I, J);
impl_tuple_data!(A, B, C, D, E, F, G, H, I, J, K);
impl_tuple_data!(A, B, C, D, E, F, G, H, I, J, K, L);
