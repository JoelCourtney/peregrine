use std::{path::PathBuf, sync::Arc};

macro_rules! impl_data {
    ($($t:ty),*) => {
        $(
            impl Data for $t {}
        )*
    };
}

pub trait Data: Clone + Send + Sync + 'static {}

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
    PathBuf
);

impl<T: Data> Data for Box<T> {}
impl<T: Data> Data for Vec<T> {}
impl<T: Data> Data for Arc<T> {}
