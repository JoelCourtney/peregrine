#[macro_export]
macro_rules! initial_conditions {
    ($($res:ident : $val:expr),*$(,)?) => {
        $crate::internal::operation::initial_conditions::InitialConditions::new()
            $(.insert::<$res>($val))*
    };
}
