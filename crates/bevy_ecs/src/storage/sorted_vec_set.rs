use core::cmp::Ordering;
use smallvec::SmallVec;

/// Stores a sorted list of indices with quick implementation for union, difference, intersection.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SortedVecSet<const N: usize>(SmallVec<[usize; N]>);

impl<const N: usize> IntoIterator for SortedVecSet<N> {
    type Item = usize;
    type IntoIter = <SmallVec<[usize; N]> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<const N: usize> Default for SortedVecSet<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> SortedVecSet<N> {
    /// Construct an empty vector
    pub(crate) const fn new() -> Self {
        Self(SmallVec::new_const())
    }

    /// Construct a new `SortedSmallVec` from a `Vec<usize>`.
    ///
    /// Elements are copied and put in a sorted order if the original `Vec` isn't ordered.
    /// Duplicates are removed.
    #[allow(dead_code)]
    pub(crate) fn from_vec(vec: Vec<usize>) -> Self {
        let mut sorted_vec = Self(SmallVec::with_capacity(vec.len()));
        for value in vec {
            sorted_vec.insert(value);
        }
        sorted_vec
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().copied()
    }

    /// Insert the value if it's not already present in the vector.
    /// Maintains a sorted order.
    pub(crate) fn insert(&mut self, index: usize) {
        match self.0.binary_search(&index) {
            // element already present in the vector
            Ok(_) => {}
            Err(pos) => {
                self.0.insert(pos, index);
            }
        }
    }

    /// Removes a value if it's present in the vector
    pub(crate) fn remove(&mut self, index: usize) {
        if let Ok(pos) = self.0.binary_search(&index) {
            self.0.remove(pos);
        }
    }

    /// Returns true if the vector contains the value.
    pub(crate) fn contains(&self, index: usize) -> bool {
        self.0.binary_search(&index).is_ok()
    }

    /// Returns true if the vector is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Empties the contents of the vector
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    /// Returns the number of elements in the vector.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn difference_with(&mut self, other: &Self) {
        let mut j = 0;
        self.0.retain(|current| {
            // Advance past any smaller elements in other
            while j < other.len() && other.0[j] < *current {
                j += 1;
            }
            // It's only in the difference if it's not in other,
            // and this is the only place in other it could be
            j < other.len() && !(other.0[j] == *current)
        });
    }

    pub(crate) fn intersect_with(&mut self, other: &Self) {
        let mut j = 0;
        self.0.retain(|current| {
            // Advance past any smaller elements in other
            while j < other.len() && other.0[j] < *current {
                j += 1;
            }
            // It's only in the intersection if it's in other,
            // and this is the only place in other it could be
            j < other.len() && other.0[j] == *current
        });
    }

    /// Adds all the elements from `other` into this vector. (skipping duplicates)
    pub(crate) fn union_with(&mut self, other: &Self) {
        let mut i = 0;
        let mut j = 0;
        while i < self.len() && j < other.len() {
            match self.0[i].cmp(&other.0[j]) {
                Ordering::Less => i += 1,
                Ordering::Greater => {
                    self.0.insert(i, other.0[j]);
                    j += 1;
                }
                Ordering::Equal => {
                    i += 1;
                    j += 1;
                }
            }
        }
        while j < other.len() {
            self.0.push(other.0[j]);
            j += 1;
        }
    }

    /// Returns the elements that are in both `self` and `other`.
    pub(crate) fn intersection<'a>(&'a self, other: &'a Self) -> Intersection<'a, N> {
        Intersection {
            this: self,
            other,
            i: 0,
            j: 0,
        }
    }

    /// Return the elements that are in `self` but not in `other`.
    pub(crate) fn difference<'a>(&'a self, other: &'a Self) -> Difference<'a, N> {
        Difference {
            this: self,
            other,
            i: 0,
            j: 0,
        }
    }

    /// Returns true if the two vectors have no common elements.
    pub(crate) fn is_disjoint(&self, other: &Self) -> bool {
        self.intersection(other).next().is_none()
    }

    /// Returns true if all the elements in `self` are also in `other`.
    pub(crate) fn is_subset(&self, other: &Self) -> bool {
        self.difference(other).next().is_none()
    }
}

/// Intersection between `this` and `other` sorted vectors.
pub(crate) struct Intersection<'a, const N: usize> {
    this: &'a SortedVecSet<N>,
    other: &'a SortedVecSet<N>,
    i: usize,
    j: usize,
}

impl<'a, const N: usize> Iterator for Intersection<'a, N> {
    type Item = usize;

    // We assume that both self and other are sorted and contain no duplicates
    // Returns items in sorted order without duplicates
    fn next(&mut self) -> Option<Self::Item> {
        while self.i < self.this.len() && self.j < self.other.len() {
            let val_a = self.this.0[self.i];
            let val_b = self.other.0[self.j];
            if val_a == val_b {
                self.i += 1;
                self.j += 1;
                return Some(val_a);
            } else if val_a < val_b {
                self.i += 1;
            } else {
                self.j += 1;
            }
        }
        return None;
    }
}

impl<'a, const N: usize> From<Intersection<'a, N>> for SortedVecSet<N> {
    fn from(intersection: Intersection<'a, N>) -> Self {
        SortedVecSet(SmallVec::from_iter(intersection))
    }
}

/// Difference between `this` and `other` sorted vector sets. this - other.
pub(crate) struct Difference<'a, const N: usize> {
    this: &'a SortedVecSet<N>,
    other: &'a SortedVecSet<N>,
    i: usize,
    j: usize,
}

impl<'a, const N: usize> Iterator for Difference<'a, N> {
    type Item = usize;

    // We assume that both self and other are sorted and contain no duplicates
    // Returns items in sorted order without duplicates
    fn next(&mut self) -> Option<Self::Item> {
        while self.i < self.this.len() && self.j < self.other.len() {
            let val_a = self.this.0[self.i];
            let val_b = self.other.0[self.j];
            if val_a == val_b {
                self.i += 1;
                self.j += 1;
            } else if val_a < val_b {
                self.i += 1;
                return Some(val_a);
            } else {
                self.j += 1;
            }
        }
        if self.i < self.this.len() {
            let val_a = self.this.0[self.i];
            self.i += 1;
            return Some(val_a);
        }
        return None;
    }
}

impl<'a, const N: usize> From<Difference<'a, N>> for SortedVecSet<N> {
    fn from(difference: Difference<'a, N>) -> Self {
        SortedVecSet(SmallVec::from_iter(difference))
    }
}
