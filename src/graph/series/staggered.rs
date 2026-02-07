use std::{collections::BTreeMap, ops::Bound};

pub(crate) struct StaggeredMap<I, T> {
    map: BTreeMap<I, T>,
    delayed: Vec<(I, T)>,
    highest: Option<I>,
    all_sorted: bool,
}

impl<I, T> Default for StaggeredMap<I, T> {
    fn default() -> Self {
        Self {
            map: BTreeMap::new(),
            delayed: Vec::new(),
            highest: None,
            all_sorted: true,
        }
    }
}

impl<I, T> StaggeredMap<I, T> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<I: Ord + Clone, T> StaggeredMap<I, T> {
    pub fn insert(&mut self, key: I, value: T) {
        if self.all_sorted
            && self
                .delayed
                .last()
                .map(|(i, _)| i >= &key)
                .or(self.highest.as_ref().map(|i| i >= &key))
                .unwrap_or(false)
        {
            self.all_sorted = false;
        }
        if self.highest.is_none() || self.highest.as_ref().unwrap() < &key {
            self.highest = Some(key.clone());
        }
        self.delayed.push((key, value));
    }

    fn sort(&mut self) {
        if !self.all_sorted {
            self.map
                .append(&mut BTreeMap::from_iter(self.delayed.drain(..)));
            self.all_sorted = true;
        }
    }

    pub fn remove(&mut self, key: &I) -> Option<T> {
        self.sort();
        self.map.remove(key).or_else(|| {
            self.delayed
                .binary_search_by_key(&key, |(k, _)| k)
                .ok()
                .map(|i| self.delayed.remove(i).1)
        })
    }

    pub fn get_before_mut(&mut self, key: I, inclusive: bool) -> Option<(&I, &mut T)> {
        self.sort();

        let result = match self.delayed.binary_search_by_key(&&key, |(k, _)| k) {
            Ok(i) if inclusive => {
                let entry = &mut self.delayed[i];
                Some((&entry.0, &mut entry.1))
            }
            Ok(i) | Err(i) if i > 0 => {
                let entry = &mut self.delayed[i - 1];
                Some((&entry.0, &mut entry.1))
            }
            _ => None,
        };

        if result.is_some() {
            return result;
        }

        let range = if inclusive {
            (Bound::Unbounded, Bound::Included(key))
        } else {
            (Bound::Unbounded, Bound::Excluded(key))
        };

        self.map.range_mut(range).next_back()
    }
}

impl<I, T> StaggeredMap<I, T> {
    pub fn into_iter_unordered(self) -> impl DoubleEndedIterator {
        self.map.into_iter().chain(self.delayed)
    }
}
