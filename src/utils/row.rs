use std::ops::{Deref, DerefMut};

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Row<E> {
    pub row: usize,
    pub entry: E,
}

impl<E> Row<E> {
    pub const fn new(idx: usize, entry: E) -> Self {
        Self { row: idx, entry }
    }
}

impl<E> Deref for Row<E> {
    type Target = E;

    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}

impl<E> DerefMut for Row<E> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entry
    }
}

impl<E> AsRef<E> for Row<E> {
    fn as_ref(&self) -> &E {
        &self.entry
    }
}

impl<E: Clone> From<Row<&E>> for Row<E> {
    fn from(row: Row<&E>) -> Self {
        Self {
            row: row.row,
            entry: row.entry.clone(),
        }
    }
}

impl<E: Clone> From<&Row<&E>> for Row<E> {
    fn from(row: &Row<&E>) -> Self {
        Self {
            row: row.row,
            entry: row.entry.clone(),
        }
    }
}

impl<E: Clone> From<&Row<E>> for Row<E> {
    fn from(row: &Row<E>) -> Self {
        Self {
            row: row.row,
            entry: row.entry.clone(),
        }
    }
}

impl<E> From<(usize, E)> for Row<E> {
    fn from((idx, entry): (usize, E)) -> Self {
        Self { row: idx, entry }
    }
}
