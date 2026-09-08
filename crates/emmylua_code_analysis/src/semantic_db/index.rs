//! Minimal bucket lookup primitive (built once; O(log n) binary search).

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Bucket<K> {
    pub key: K,
    pub indices: Vec<u32>,
}

pub fn build_buckets<K: Ord + Clone>(mut entries: Vec<(K, u32)>) -> Vec<Bucket<K>> {
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut buckets: Vec<Bucket<K>> = Vec::new();
    for (key, index) in entries {
        if let Some(last) = buckets.last_mut()
            && last.key == key
        {
            last.indices.push(index);
            continue;
        }
        buckets.push(Bucket {
            key,
            indices: vec![index],
        });
    }
    buckets
}

pub fn find_bucket<'a, K: Ord>(buckets: &'a [Bucket<K>], key: &K) -> Option<&'a [u32]> {
    let index = buckets
        .binary_search_by(|bucket| bucket.key.cmp(key))
        .ok()?;
    Some(&buckets[index].indices)
}
