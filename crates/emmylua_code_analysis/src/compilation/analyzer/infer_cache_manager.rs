use std::collections::HashMap;

use crate::{FileId, LuaAnalysisPhase, semantic::LuaInferCache};

#[derive(Debug, Default)]
pub struct InferCacheManager {
    infer_map: HashMap<FileId, LuaInferCache>,
}

impl InferCacheManager {
    pub fn new() -> Self {
        InferCacheManager {
            infer_map: HashMap::new(),
        }
    }

    pub fn get_infer_cache(&mut self, file_id: FileId) -> &mut LuaInferCache {
        self.infer_map.entry(file_id).or_insert_with(|| {
            LuaInferCache::new(
                file_id,
                crate::CacheOptions {
                    analysis_phase: LuaAnalysisPhase::Ordered,
                },
            )
        })
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
