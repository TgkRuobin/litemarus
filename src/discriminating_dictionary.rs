//! Matches `litemapy.storage.DiscriminatingDictionary`.

use std::collections::hash_map::Keys;
use std::collections::HashMap;
use std::hash::Hash;

use crate::error::DiscriminationError;

pub type ValidatorFn<K, V> = dyn Fn(&K, &V) -> Result<(), DiscriminationError>;

pub struct DiscriminatingDictionary<K, V>
where
    K: Eq + Hash + Clone,
{
    inner: HashMap<K, V>,
    validator: Box<ValidatorFn<K, V>>,
    pub on_add: Option<Box<dyn FnMut(&K, &V)>>,
    pub on_remove: Option<Box<dyn FnMut(&K, &V)>>,
}

impl<K, V> DiscriminatingDictionary<K, V>
where
    K: Eq + Hash + Clone,
{
    pub fn new<F>(validator: F) -> Self
    where
        F: Fn(&K, &V) -> Result<(), DiscriminationError> + 'static,
    {
        Self {
            inner: HashMap::new(),
            validator: Box::new(validator),
            on_add: None,
            on_remove: None,
        }
    }

    pub fn with_hooks<F, A, R>(validator: F, onadd: A, onremove: R) -> Self
    where
        F: Fn(&K, &V) -> Result<(), DiscriminationError> + 'static,
        A: FnMut(&K, &V) + 'static,
        R: FnMut(&K, &V) + 'static,
    {
        Self {
            inner: HashMap::new(),
            validator: Box::new(validator),
            on_add: Some(Box::new(onadd)),
            on_remove: Some(Box::new(onremove)),
        }
    }

    pub fn from_map<F>(validator: F, entries: HashMap<K, V>) -> Result<Self, DiscriminationError>
    where
        F: Fn(&K, &V) -> Result<(), DiscriminationError> + 'static,
    {
        let mut s = Self::new(validator);
        for (k, v) in entries {
            s.insert(k, v)?;
        }
        Ok(s)
    }

    #[allow(dead_code)]
    fn notify_add(&mut self, k: &K, v: &V) {
        if let Some(h) = self.on_add.as_mut() {
            (h)(k, v)
        }
    }

    fn notify_rm(&mut self, k: &K, v: &V) {
        if let Some(h) = self.on_remove.as_mut() {
            (h)(k, v)
        }
    }

    pub fn validate(&mut self, key: &K, item: &V) -> Result<(), DiscriminationError> {
        (self.validator)(key, item)
    }

    pub fn insert(&mut self, key: K, item: V) -> Result<Option<V>, DiscriminationError> {
        (self.validator)(&key, &item)?;
        let had = self.inner.contains_key(&key);
        let old = if had {
            self.inner.remove(&key)
        } else {
            None
        };
        if let Some(ref o) = old {
            self.notify_rm(&key, o);
        }
        self.inner.insert(key.clone(), item);
        if let Some(h) = self.on_add.as_mut() {
            let v = self.inner.get(&key).expect("just inserted");
            (h)(&key, v);
        }
        Ok(old)
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.inner.get_mut(key)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn keys(&self) -> Keys<'_, K, V> {
        self.inner.keys()
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.inner.values()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter()
    }

    pub fn remove(&mut self, key: &K) -> Result<Option<V>, DiscriminationError> {
        let v = self.inner.remove(key);
        if let Some(ref val) = v {
            self.notify_rm(key, val);
        }
        Ok(v)
    }

    pub fn clear(&mut self) {
        let copy: Vec<(K, V)> = self.inner.drain().collect();
        self.inner.clear();
        for (k, v) in copy {
            self.notify_rm(&k, &v);
        }
    }

    pub fn update(
        &mut self,
        mut other: DiscriminatingDictionary<K, V>,
    ) -> Result<(), DiscriminationError> {
        let entries: Vec<(K, V)> = other.inner.drain().collect();
        for (k, v) in entries {
            self.insert(k, v)?;
        }
        Ok(())
    }
}
