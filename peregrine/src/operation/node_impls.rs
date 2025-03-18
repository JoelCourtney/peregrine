#[cfg(feature = "pregenerate_nodes")]
pub use impls::*;

#[cfg(feature = "pregenerate_nodes")]
mod impls {
    use crate as peregrine;
    use crate::macro_prelude;
    use peregrine_macros::{impl_nodes, impl_read_structs, impl_write_structs};

    impl_read_structs!(0);
    impl_read_structs!(1);
    impl_read_structs!(2);
    impl_read_structs!(3);
    impl_read_structs!(4);
    impl_read_structs!(5);
    impl_read_structs!(6);
    impl_read_structs!(7);
    impl_read_structs!(8);
    impl_read_structs!(9);
    impl_read_structs!(10);

    impl_write_structs!(1);
    impl_write_structs!(2);
    impl_write_structs!(3);
    impl_write_structs!(4);
    impl_write_structs!(5);
    impl_write_structs!(6);
    impl_write_structs!(7);
    impl_write_structs!(8);
    impl_write_structs!(9);
    impl_write_structs!(10);

    impl_nodes!(5);
}
