use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    sync::{LazyLock, Mutex},
};

use crate::core::{StaticLayerSpec, UiId};

static RASTER_CACHE: LazyLock<Mutex<HashMap<String, StaticLayerRaster>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn clear() {
    RASTER_CACHE
        .lock()
        .expect("static layer raster cache poisoned")
        .clear();
}

#[derive(Clone)]
pub struct StaticLayerRaster {
    pub width: i32,
    pub height: i32,
    pub premultiplied_bgra: Vec<u8>,
}

pub fn cache_key(
    id: &UiId,
    spec: &StaticLayerSpec,
    width: i32,
    height: i32,
    child_signature: u64,
) -> String {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    child_signature.hash(&mut hasher);
    spec.cache_signature().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn load(key: &str) -> Option<StaticLayerRaster> {
    RASTER_CACHE
        .lock()
        .expect("static layer raster cache poisoned")
        .get(key)
        .cloned()
}

pub fn store(key: &str, raster: &StaticLayerRaster) {
    RASTER_CACHE
        .lock()
        .expect("static layer raster cache poisoned")
        .insert(key.to_owned(), raster.clone());
}
