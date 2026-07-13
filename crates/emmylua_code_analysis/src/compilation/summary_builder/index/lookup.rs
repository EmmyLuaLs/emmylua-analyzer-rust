#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaLookupBucket<K> {
    pub key: K,
    pub indices: Vec<usize>,
}

pub fn build_lookup_buckets<K>(mut entries: Vec<(K, usize)>) -> Vec<SalsaLookupBucket<K>>
where
    K: Ord + Clone,
{
    entries.sort_by(|left, right| left.0.cmp(&right.0));

    let mut buckets: Vec<SalsaLookupBucket<K>> = Vec::new();
    for (key, index) in entries {
        if let Some(bucket) = buckets.last_mut()
            && bucket.key == key
        {
            bucket.indices.push(index);
            continue;
        }

        buckets.push(SalsaLookupBucket {
            key,
            indices: vec![index],
        });
    }

    buckets
}

pub fn find_bucket_indices<'a, K>(
    buckets: &'a [SalsaLookupBucket<K>],
    key: &K,
) -> Option<&'a [usize]>
where
    K: Ord,
{
    let index = buckets
        .binary_search_by(|bucket| bucket.key.cmp(key))
        .ok()?;
    Some(buckets[index].indices.as_slice())
}
