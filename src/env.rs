use std::mem::transmute;

use bumpalo::Bump;
use typed_arena::Arena;

/// An arena allocator created so that multiple node data
/// structures can share the same lifetime.
///
/// Calling [borrow][Env::borrow] creates a new environment that simply
/// borrows from self.
///
/// When calling [alloc][Env::alloc], it automatically detects types
/// that need to be dropped, and drops them when the original
/// env instance goes out of scope.
pub enum Env<'e> {
    Owned {
        #[doc(hidden)]
        bump: Bump,
        #[doc(hidden)]
        to_drop: Arena<*mut (dyn ActuallyAnyForReal + 'e)>,
    },
    Borrowed(#[doc(hidden)] &'e Env<'e>),
}

#[doc(hidden)]
pub trait ActuallyAnyForReal {}
impl<T> ActuallyAnyForReal for T {}

impl<'e> Env<'e> {
    pub fn new() -> Env<'static> {
        Env::Owned {
            bump: Bump::new(),
            to_drop: Arena::new(),
        }
    }

    pub fn borrow(&'e self) -> Env<'e> {
        match self {
            Env::Owned { .. } => Env::Borrowed(self),
            Env::Borrowed(e) => Env::Borrowed(e),
        }
    }

    pub fn alloc<T>(&self, value: T) -> &'e mut T {
        match self {
            Env::Borrowed(env) => env.alloc(value),
            Env::Owned { bump, to_drop } => {
                if !std::mem::needs_drop::<T>() {
                    let result = bump.alloc(value);
                    unsafe { transmute::<&mut T, &mut T>(result) }
                } else {
                    let r = bump.alloc(value) as *mut T;
                    let result = unsafe { &mut *r };
                    to_drop.alloc(r);
                    result
                }
            }
        }
    }
}

impl Drop for Env<'_> {
    fn drop(&mut self) {
        if let Env::Owned { to_drop, .. } = self {
            for ptr in to_drop.iter_mut() {
                unsafe {
                    ptr.drop_in_place();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Env;
    use std::rc::Rc;

    #[test]
    fn test_drop() {
        let rc = Rc::new(5);
        let env = Env::new();

        assert_eq!(Rc::strong_count(&rc), 1);

        env.alloc(rc.clone());
        assert_eq!(Rc::strong_count(&rc), 2);

        drop(env);
        assert_eq!(Rc::strong_count(&rc), 1);
    }

    #[test]
    fn test_no_drop() {
        let env = Env::new();
        env.alloc(5);

        let Env::Owned { to_drop, .. } = &env else {
            unreachable!()
        };

        assert_eq!(to_drop.len(), 0)
    }
}
