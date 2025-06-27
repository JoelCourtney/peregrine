#[macro_export]
macro_rules! initial_conditions {
    ($($res:ident $(: $val:expr)?),*$(,)?) => {
        $crate::internal::operation::initial_conditions::InitialConditions::new()
            $(.insert::<$res>(
                $crate::internal::macro_prelude::spez::spez! {
                    for x = ($res::Unit, $($val)?);
                    match ($res, <$res as $crate::public::resource::Resource>::Data) -> <$res as $crate::public::resource::Resource>::Data {
                        x.1
                    }
                    match<R: $crate::public::resource::Resource> (R,) where R::Data: Default -> R::Data {
                        Default::default()
                    }
                    match<T> T {
                        panic!("Initial condition must either be given a value or implement Default.")
                    }
                }
            ))*
    };
}
