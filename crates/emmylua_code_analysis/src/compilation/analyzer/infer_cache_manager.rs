use hashbrown::HashMap;

use crate::{CacheOptions, FileId, LuaAnalysisPhase, semantic::LuaInferCache};

#[derive(Debug, Default)]
pub struct InferCacheManager {
    infer_map: HashMap<FileId, LuaInferCache>,
    cache_options: CacheOptions,
}

impl InferCacheManager {
    pub fn new(cache_options: CacheOptions) -> Self {
        InferCacheManager {
            infer_map: HashMap::new(),
            cache_options,
        }
    }

    pub fn get_infer_cache(&mut self, file_id: FileId) -> &mut LuaInferCache {
        let mut cache_options = self.cache_options;
        cache_options.analysis_phase = LuaAnalysisPhase::Ordered;
        self.infer_map
            .entry(file_id)
            .or_insert_with(|| LuaInferCache::new(file_id, cache_options))
    }

    pub fn set_force(&mut self) {
        for (_, infer_cache) in self.infer_map.iter_mut() {
            infer_cache.set_phase(LuaAnalysisPhase::Force);
        }
    }

    pub fn clear(&mut self) {
        for (_, infer_cache) in self.infer_map.iter_mut() {
            infer_cache.clear();
        }
    }
}
