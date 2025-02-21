#[macro_export]
macro_rules! resource {
    ($vis:vis static $name:ident: $ty:ty) => {
        #[derive(Debug, Serialize, Deserialize)]
        #[allow(non_camel_case_types)]
        $vis enum $name {}
        impl<'h> $crate::Resource<'h> for $name {
            const STATIC: bool = true;
            type Read = $ty;
            type Write = $ty;
            type History = $crate::CopyHistory<$ty>;
        }
    };

    ($vis:vis static ref $name:ident: $ty:ty) => {
        #[derive(Debug, Serialize, Deserialize)]
        #[allow(non_camel_case_types)]
        $vis enum $name {}
        impl<'h> $crate::Resource<'h> for $name {
            const STATIC: bool = true;
            type Read = &'h <$ty as std::ops::Deref>::Target;
            type Write = $ty;
            type History = $crate::DerefHistory<$ty>;
        }
    };
}
