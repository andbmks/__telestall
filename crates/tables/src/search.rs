use std::collections::HashMap;

pub trait Searchable: Sized {
    fn fill_haystack(&self, query: &mut Searcher);
}

pub struct Searcher {
    groups: HashMap<String, String>,
}

impl Searcher {
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
        }
    }

    pub fn write(&mut self, group: String, haystack: String) {
        self.groups
            .entry(group)
            .or_default()
            .extend(haystack.chars());
    }

    pub fn write_many(&mut self, group: String, haystack: impl IntoIterator<Item = String>) {
        self.groups.entry(group).or_default().extend(haystack);
    }

    pub fn search_one(&self, group: &str, query: &str) -> bool {
        self.groups
            .get(group)
            .is_some_and(|haystack| haystack.contains(query))
    }

    pub fn search<'a>(&self, group: &str, mut query: impl Iterator<Item = &'a String>) -> bool {
        self.groups
            .get(group)
            .is_some_and(|haystack| query.any(|word| haystack.contains(word)))
    }
}

impl<E: Searchable> From<E> for Searcher {
    fn from(entry: E) -> Self {
        let mut query = Self::new();
        entry.fill_haystack(&mut query);
        query
    }
}
