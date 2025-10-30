use std::cell::Cell;

/// A lock-free atomic-free synchronous bool that starts false and
/// can only be set to true.
pub(crate) struct OnceBool {
    state: Cell<bool>,
}

impl OnceBool {
    pub fn new() -> Self {
        Self {
            state: Cell::new(false),
        }
    }

    pub fn set(&self) {
        self.state.set(true);
    }

    pub fn get(self) -> bool {
        self.state.get()
    }
}

impl Default for OnceBool {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Sync for OnceBool {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_once_flag() {
        let flag = OnceBool::new();
        assert!(!flag.get());

        let flag = OnceBool::new();
        flag.set();
        assert!(flag.get());
    }
}
