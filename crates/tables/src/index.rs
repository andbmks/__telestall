use std::{
    collections::{hash_map, HashMap},
    convert::Infallible,
    hash::Hash,
};

use async_trait::async_trait;

use crate::{TableClear, TableExtend, TableUpdate};

pub struct Index<K, E, V = E> {
    map: HashMap<K, Vec<(usize, V)>>,
    get_key: fn(usize, &E) -> K,
}

impl<K, E, V: From<E>> Index<K, E, V>
where
    K: Hash + PartialEq + Ord,
{
    pub fn new(get_key: fn(usize, &E) -> K) -> Self {
        Self {
            map: HashMap::new(),
            get_key,
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.get_with_row(key).map(|(_, v)| v)
    }

    pub fn get_with_row(&self, key: &K) -> Option<&(usize, V)> {
        self.map.get(key).and_then(|v| v.first())
    }

    pub fn group(&self, key: &K) -> Option<&Vec<(usize, V)>> {
        self.map.get(key)
    }

    pub fn all(&self) -> hash_map::Iter<'_, K, Vec<(usize, V)>> {
        self.map.iter()
    }

    fn _extend<T>(&mut self, entries: T)
    where
        T: IntoIterator<Item = E>,
    {
        for (row, entry) in entries.into_iter().enumerate() {
            let key = (self.get_key)(row, &entry);
            self.map
                .entry(key)
                .or_insert_with(Vec::new)
                .push((row, entry.into()));
        }
    }

    fn _update<T>(&mut self, from_row: usize, entries: T)
    where
        T: IntoIterator<Item = E>,
    {
        for (mut row, entry) in entries.into_iter().enumerate() {
            row += from_row;
            let key = (self.get_key)(row, &entry);

            match self.map.entry(key) {
                hash_map::Entry::Occupied(e) => {
                    let vec = e.into_mut();
                    let value_ref = vec.iter_mut().find(|(i, _)| *i == row);

                    match value_ref {
                        Some((_, value_ref)) => *value_ref = entry.into(),
                        None => vec.push((row, entry.into())),
                    }
                }
                hash_map::Entry::Vacant(e) => e.insert(vec![]).push((row, entry.into())),
            }
        }
    }
}

#[async_trait]
impl<K, E, V> TableExtend<E> for Index<K, E, V>
where
    K: Send + Hash + PartialEq + Ord,
    E: Send + Clone,
    V: From<E> + Send,
{
    type Ok = ();
    type Error = Infallible;

    async fn extend<'a, T>(&'a mut self, entries: T) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = &'a E> + Send,
        E: 'a,
    {
        self._extend(entries.into_iter().cloned());
        Ok(())
    }

    async fn extend_owned<T>(&mut self, entries: T) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = E> + Send,
    {
        self._extend(entries.into_iter());
        Ok(())
    }
}

#[async_trait]
impl<K, E, V> TableUpdate<E> for Index<K, E, V>
where
    K: Send + Hash + PartialEq + Ord,
    E: Send + Clone,
    V: From<E> + Send,
{
    type Ok = ();
    type Error = Infallible;

    async fn update<'a, T>(
        &'a mut self,
        from_row: usize,
        entries: T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = &'a E> + Send,
        E: 'a,
    {
        self._update(from_row, entries.into_iter().cloned());
        Ok(())
    }

    async fn update_owned<T>(
        &mut self,
        from_row: usize,
        entries: T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: IntoIterator<Item = E> + Send,
        E: Sync,
    {
        self._update(from_row, entries.into_iter());
        Ok(())
    }
}

#[async_trait]
impl<K, E, V> TableClear for Index<K, E, V>
where
    K: Send,
    E: Send,
    V: Send,
{
    type Error = Infallible;

    async fn clear(&mut self) -> Result<(), Self::Error> {
        self.map.clear();
        Ok(())
    }
}
