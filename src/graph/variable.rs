use std::sync::Arc;

use parking_lot::RwLock;

use crate::{Callback, Ctx, Data, Upstream, cache::Cache, node::Node};

pub struct Var<'a, O: Send + 'static> {
    cell: RwLock<Arc<dyn Upstream<Output = O> + 'a>>,
    cache: Cache<()>,
}

impl<'a, O: Data> Var<'a, O> {
    pub fn new(node: impl Upstream<Output = O> + 'a) -> Var<'a, O> {
        Var {
            cell: RwLock::new(Arc::new(node)),
            cache: Cache::new(),
        }
    }

    pub fn set(&self, node: impl Upstream<Output = O> + 'a) {
        let mut write = self.cell.write();
        let new_node = Arc::new(node);
        self.cache.invalidate();
        *write = new_node;
    }

    pub fn freeze(&self) -> Node<Arc<dyn Upstream<Output = O> + 'a>> {
        Node((*self.cell.read()).clone())
    }
}

impl<O: Data> Upstream for Var<'_, O> {
    type Output = O;

    fn request<'s>(&self, ctx: Ctx<'_, 's>, callback: Callback<'s, O>)
    where
        Self: 's,
    {
        let sender = self.cache.get_invalidator_sender();
        let callback = callback.map(|mut c| {
            c.push_sender(sender, false);
            c
        });
        let cell = self.cell.read();
        cell.request(ctx, callback);
    }
}

impl<O: Upstream<Output = O> + Default + Send + 'static> Default for Var<'_, O> {
    fn default() -> Self {
        Var {
            cell: RwLock::new(Arc::new(O::default())),
            cache: Cache::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate as peregrine;
    use crate::*;

    #[test]
    fn var() {
        let var = Var::new(0);
        assert_eq!(run(&var), 0);

        var.set(7);
        assert_eq!(run(&var), 7);
    }

    #[test]
    fn upstream_var() {
        let x = op!(i!(1));
        let y = Var::new(op! { i!(&x) + 1 });

        assert_eq!(run(y), 2);
    }

    #[test]
    fn mutate_upstream_var() {
        let x = Var::new(0);
        let y = op! { i!(&x) + 1 };

        assert_eq!(run(&y), 1);

        x.set(10);
        assert_eq!(run(y), 11);
    }

    #[test]
    fn freeze() {
        let x = Var::new(0);

        assert_eq!(run(&x), 0);

        let frozen = x.freeze();
        x.set(10);
        assert_eq!(run(&x), 10);

        assert_eq!(run(frozen), 0);
    }

    #[test]
    fn freeze_drop() {
        let payload = Arc::new(());

        let x = Var::new(payload.clone());
        assert_eq!(Arc::strong_count(&payload), 2);

        x.set(Arc::new(()));
        assert_eq!(Arc::strong_count(&payload), 1);

        x.set(payload.clone());
        let frozen = x.freeze();
        assert_eq!(Arc::strong_count(&payload), 2);
        x.set(payload.clone());
        assert_eq!(Arc::strong_count(&payload), 3);

        drop(frozen);
        drop(x);

        assert_eq!(Arc::strong_count(&payload), 1);
    }
}
