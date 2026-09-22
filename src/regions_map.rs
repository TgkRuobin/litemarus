//! Regions container — matches `Schematic.regions` dict behavior for bounding boxes.

use indexmap::IndexMap;

use crate::region::Region;

#[derive(Clone, Debug)]
pub struct RegionsMap {
    pub(crate) inner: IndexMap<String, Region>,
    x_min: Option<i32>,
    x_max: Option<i32>,
    y_min: Option<i32>,
    y_max: Option<i32>,
    z_min: Option<i32>,
    z_max: Option<i32>,
}

impl RegionsMap {
    pub(crate) fn new_empty() -> Self {
        Self {
            inner: IndexMap::new(),
            x_min: None,
            x_max: None,
            y_min: None,
            y_max: None,
            z_min: None,
            z_max: None,
        }
    }

    pub(crate) fn from_entries(entries: IndexMap<String, Region>) -> Self {
        let mut s = Self::new_empty();
        s.inner = entries;
        s.recompute_enclosure();
        s
    }

    /// `sch.regions[k] = reg` — returns previous region if key existed.
    pub fn insert(&mut self, name: String, region: Region) -> Option<Region> {
        let old = self.inner.shift_remove(&name);
        if old.is_some() {
            self.recompute_enclosure();
        }
        // Bounds are read straight off the region; cloning it here used to copy
        // the whole block array for nothing.
        self.merge_bounds(&region);
        self.inner.insert(name, region);
        old
    }

    pub fn get(&self, key: &str) -> Option<&Region> {
        self.inner.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Region> {
        self.inner.get_mut(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<Region> {
        let v = self.inner.shift_remove(key);
        if let Some(ref rv) = v {
            self.maybe_recompute_on_remove(rv);
        }
        v
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Region)> {
        self.inner.iter()
    }

    /// Mutable iteration. Region positions/sizes are immutable, so the cached
    /// enclosure bounds stay valid.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&String, &mut Region)> {
        self.inner.iter_mut()
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.inner.keys()
    }

    pub fn values(&self) -> impl Iterator<Item = &Region> {
        self.inner.values()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Region> {
        self.inner.values_mut()
    }

    fn merge_bounds(&mut self, region: &Region) {
        self.x_min = Some(match self.x_min {
            None => region.min_schem_x(),
            Some(v) => v.min(region.min_schem_x()),
        });
        self.x_max = Some(match self.x_max {
            None => region.max_schem_x(),
            Some(v) => v.max(region.max_schem_x()),
        });
        self.y_min = Some(match self.y_min {
            None => region.min_schem_y(),
            Some(v) => v.min(region.min_schem_y()),
        });
        self.y_max = Some(match self.y_max {
            None => region.max_schem_y(),
            Some(v) => v.max(region.max_schem_y()),
        });
        self.z_min = Some(match self.z_min {
            None => region.min_schem_z(),
            Some(v) => v.min(region.min_schem_z()),
        });
        self.z_max = Some(match self.z_max {
            None => region.max_schem_z(),
            Some(v) => v.max(region.max_schem_z()),
        });
    }

    fn maybe_recompute_on_remove(&mut self, removed: &Region) {
        let touch = self.x_min == Some(removed.min_schem_x())
            || self.x_max == Some(removed.max_schem_x())
            || self.y_min == Some(removed.min_schem_y())
            || self.y_max == Some(removed.max_schem_y())
            || self.z_min == Some(removed.min_schem_z())
            || self.z_max == Some(removed.max_schem_z());
        if touch {
            self.recompute_enclosure();
        }
    }

    fn recompute_enclosure(&mut self) {
        let mut bounds: Option<(i32, i32, i32, i32, i32, i32)> = None;
        for r in self.inner.values() {
            let here = (
                r.min_schem_x(),
                r.max_schem_x(),
                r.min_schem_y(),
                r.max_schem_y(),
                r.min_schem_z(),
                r.max_schem_z(),
            );
            bounds = Some(match bounds {
                None => here,
                Some((x0, x1, y0, y1, z0, z1)) => (
                    x0.min(here.0),
                    x1.max(here.1),
                    y0.min(here.2),
                    y1.max(here.3),
                    z0.min(here.4),
                    z1.max(here.5),
                ),
            });
        }
        let (x_min, x_max, y_min, y_max, z_min, z_max) = match bounds {
            Some((x0, x1, y0, y1, z0, z1)) => {
                (Some(x0), Some(x1), Some(y0), Some(y1), Some(z0), Some(z1))
            }
            None => (None, None, None, None, None, None),
        };
        self.x_min = x_min;
        self.x_max = x_max;
        self.y_min = y_min;
        self.y_max = y_max;
        self.z_min = z_min;
        self.z_max = z_max;
    }

    pub fn width(&self) -> i32 {
        match (self.x_min, self.x_max) {
            (Some(mi), Some(ma)) => ma - mi + 1,
            _ => 0,
        }
    }

    pub fn height(&self) -> i32 {
        match (self.y_min, self.y_max) {
            (Some(mi), Some(ma)) => ma - mi + 1,
            _ => 0,
        }
    }

    pub fn length(&self) -> i32 {
        match (self.z_min, self.z_max) {
            (Some(mi), Some(ma)) => ma - mi + 1,
            _ => 0,
        }
    }
}
