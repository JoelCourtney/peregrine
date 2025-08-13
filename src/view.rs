macro_rules! impl_view_for_copy {
    ($($t:ty),*) => {
        $(
            impl View for $t {
                type Result = $t;

                fn view(&self) -> Self::Result {
                    *self
                }
            }
        )*
    };
}

pub trait View: Send + Sync + 'static {
    type Result: Copy + Send;

    fn view(&self) -> Self::Result;
}

impl_view_for_copy!(u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64, &'static str, bool, char);
