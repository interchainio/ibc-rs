use alloc::collections::BTreeMap as HashMap;
use core::cmp::Ord;
use core::hash::Hash;

/// A change between two dictionaries.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Change<K> {
    /// The entry with this key was added.
    Added(K),
    /// The entry with this key was updated.
    Updated(K),
    /// The entry with this key was removed.
    Removed(K),
}

impl<K> Change<K> {
    /// Return the key affected by this change.
    pub fn key(&self) -> &K {
        match self {
            Self::Added(ref k) => k,
            Self::Updated(ref k) => k,
            Self::Removed(ref k) => k,
        }
    }

    /// Return the key affected by this change.
    pub fn into_key(self) -> K {
        match self {
            Self::Added(k) => k,
            Self::Updated(k) => k,
            Self::Removed(k) => k,
        }
    }
}

/// Computes the set of changes between the `prev` and `next` dictionaries.
/// An entry is deemed to have been updated when it is not equal to the original
/// value according to its `Eq` instance.
///
/// Returns a list of changes holding on to the key affected by the change.
pub fn diff<'a, K, V>(prev: &'a HashMap<K, V>, next: &'a HashMap<K, V>) -> Vec<Change<&'a K>>
where
    K: Hash + Eq + Ord,
    V: Eq,
{
    gdiff(prev, next, |a, b| a == b)
}

/// Computes the set of changes between the `prev` and `next` dictionaries.
/// An entry is deemed to have been updated when `!eq(prev_value, next_value)`.
///
/// Returns a list of changes holding on to the key affected by the change.
pub fn gdiff<'a, K, V, F>(
    prev: &'a HashMap<K, V>,
    next: &'a HashMap<K, V>,
    eq: F,
) -> Vec<Change<&'a K>>
where
    K: Hash + Eq + Ord,
    F: Fn(&'a V, &'a V) -> bool,
{
    let mut changes = Vec::new();

    for (key, value) in prev {
        if let Some(next_value) = next.get(key) {
            if !eq(value, next_value) {
                changes.push(Change::Updated(key));
            }
        } else {
            changes.push(Change::Removed(key));
        }
    }

    for key in next.keys() {
        if !prev.contains_key(key) {
            changes.push(Change::Added(key));
        }
    }

    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use Change::*;

    #[test]
    fn it_works() {
        let prev = vec![(1, 1), (2, 2), (3, 3), (4, 4)].into_iter().collect();
        let next = vec![(1, 11), (2, 2), (4, 44), (5, 5)].into_iter().collect();

        let mut diff = diff(&prev, &next);
        diff.sort();

        assert_eq!(diff, vec![Added(&5), Updated(&1), Updated(&4), Removed(&3)]);
    }
}
