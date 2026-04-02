use crate::FileId;
use hashbrown::{HashMap, HashSet};
use std::collections::VecDeque;

#[derive(Debug)]
pub struct FileDependencyRelation<'a> {
    dependencies: &'a HashMap<FileId, HashSet<FileId>>,
}

impl<'a> FileDependencyRelation<'a> {
    pub fn new(dependencies: &'a HashMap<FileId, HashSet<FileId>>) -> Self {
        Self { dependencies }
    }

    pub fn get_best_analysis_order(
        &self,
        file_ids: &[FileId],
        metas: &HashSet<FileId>,
    ) -> Vec<FileId> {
        let n = file_ids.len();
        if n < 2 {
            return file_ids.to_vec();
        }

        let file_to_idx: HashMap<FileId, usize> =
            file_ids.iter().enumerate().map(|(i, &f)| (f, i)).collect();

        let mut in_degree = vec![0usize; n];
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];

        for (idx, &file_id) in file_ids.iter().enumerate() {
            if let Some(deps) = self.dependencies.get(&file_id) {
                for &dep in deps {
                    if let Some(&dep_idx) = file_to_idx.get(&dep) {
                        adjacency[dep_idx].push(idx);
                        in_degree[idx] += 1;
                    }
                }
            }
        }
        let mut result = Vec::with_capacity(n);
        let mut queue = VecDeque::with_capacity(n);

        // 入度为0的节点，按优先级排序：meta文件优先，然后按FileId排序
        let mut zero_in_degree: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        zero_in_degree.sort_by(|&a, &b| {
            let a_is_meta = metas.contains(&file_ids[a]);
            let b_is_meta = metas.contains(&file_ids[b]);
            // meta文件优先（true > false，所以反过来比较）
            match (b_is_meta, a_is_meta) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => file_ids[a].cmp(&file_ids[b]),
            }
        });

        for idx in zero_in_degree {
            queue.push_back(idx);
        }

        while let Some(idx) = queue.pop_front() {
            result.push(file_ids[idx]);

            // 收集新的入度为0的节点
            let mut new_zero: Vec<usize> = Vec::new();
            for &neighbor in &adjacency[idx] {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    new_zero.push(neighbor);
                }
            }

            // 同样按优先级排序后加入队列
            if new_zero.len() > 1 {
                new_zero.sort_by(|&a, &b| {
                    let a_is_meta = metas.contains(&file_ids[a]);
                    let b_is_meta = metas.contains(&file_ids[b]);
                    match (b_is_meta, a_is_meta) {
                        (true, false) => std::cmp::Ordering::Greater,
                        (false, true) => std::cmp::Ordering::Less,
                        _ => file_ids[a].cmp(&file_ids[b]),
                    }
                });
            }
            for neighbor in new_zero {
                queue.push_back(neighbor);
            }
        }

        // 处理循环依赖
        if result.len() < n {
            for (idx, &deg) in in_degree.iter().enumerate() {
                if deg > 0 {
                    result.push(file_ids[idx]);
                }
            }
        }

        result
    }

    /// Get all direct and indirect dependencies for the file list
    pub fn collect_file_dependents(&self, file_ids: Vec<FileId>) -> Vec<FileId> {
        let mut reverse_map: HashMap<FileId, Vec<FileId>> = HashMap::new();
        for (&fid, deps) in self.dependencies.iter() {
            for &dep in deps {
                reverse_map.entry(dep).or_default().push(fid);
            }
        }
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();
        for file_id in file_ids {
            queue.push_back(file_id);
        }
        while let Some(file_id) = queue.pop_front() {
            if let Some(dependents) = reverse_map.get(&file_id) {
                for &d in dependents {
                    if result.insert(d) {
                        queue.push_back(d);
                    }
                }
            }
        }
        result.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_best_analysis_order() {
        let mut map = HashMap::new();
        // 文件1依赖文件2
        map.insert(FileId::new(1), {
            let mut s = HashSet::new();
            s.insert(FileId::new(2));
            s
        });
        // 文件2没有依赖
        map.insert(FileId::new(2), HashSet::new());
        let rel = FileDependencyRelation::new(&map);
        let result =
            rel.get_best_analysis_order(&[FileId::new(1), FileId::new(2)], &HashSet::default());
        // 文件2没有依赖，应该在前；文件1依赖文件2，在后
        assert_eq!(result, vec![FileId::new(2), FileId::new(1)]);
    }

    #[test]
    fn test_best_analysis_order2() {
        let mut map = HashMap::new();
        // 文件1依赖文件2和文件3
        map.insert(1.into(), {
            let mut s = HashSet::new();
            s.insert(2.into());
            s.insert(3.into());
            s
        });
        // 文件2依赖文件3
        map.insert(2.into(), {
            let mut s = HashSet::new();
            s.insert(3.into());
            s
        });
        // 文件3没有依赖
        map.insert(3.into(), HashSet::new());
        let rel = FileDependencyRelation::new(&map);
        let result =
            rel.get_best_analysis_order(&[1.into(), 2.into(), 3.into()], &HashSet::default());
        // 文件3没有依赖，应该在最前面；然后是2，最后是1
        assert_eq!(result, vec![3.into(), 2.into(), 1.into()]);
    }

    #[test]
    fn test_no_deps_files_first() {
        let mut map = HashMap::new();
        // 文件1依赖文件2
        map.insert(FileId::new(1), {
            let mut s = HashSet::new();
            s.insert(FileId::new(2));
            s
        });
        // 文件2依赖文件1（循环依赖）
        map.insert(FileId::new(2), {
            let mut s = HashSet::new();
            s.insert(FileId::new(1));
            s
        });
        // 文件3没有依赖
        map.insert(FileId::new(3), HashSet::new());
        // 文件4没有依赖
        map.insert(FileId::new(4), HashSet::new());

        let rel = FileDependencyRelation::new(&map);
        let result = rel.get_best_analysis_order(
            &[
                FileId::new(1),
                FileId::new(2),
                FileId::new(3),
                FileId::new(4),
            ],
            &HashSet::default(),
        );

        // 文件3和4没有依赖，应该在前面
        assert_eq!(result[0], FileId::new(3));
        assert_eq!(result[1], FileId::new(4));
        // 文件1和2有循环依赖，在后面
        assert!(result.contains(&FileId::new(1)));
        assert!(result.contains(&FileId::new(2)));
    }

    #[test]
    fn test_collect_file_dependents() {
        let mut deps = HashMap::new();
        deps.insert(
            FileId::new(1),
            [FileId::new(2), FileId::new(3)].iter().cloned().collect(),
        );
        deps.insert(FileId::new(2), [FileId::new(3)].iter().cloned().collect());
        deps.insert(FileId::new(3), HashSet::new());
        deps.insert(FileId::new(4), [FileId::new(3)].iter().cloned().collect());

        let rel = FileDependencyRelation::new(&deps);
        let mut result = rel.collect_file_dependents(vec![FileId::new(3)]);
        result.sort();
        assert_eq!(result, vec![FileId::new(1), FileId::new(2), FileId::new(4)]);
    }

    #[test]
    fn test_meta_files_first() {
        let mut map = HashMap::new();
        // 所有文件都没有依赖
        map.insert(FileId::new(1), HashSet::new());
        map.insert(FileId::new(2), HashSet::new());
        map.insert(FileId::new(3), HashSet::new());
        map.insert(FileId::new(4), HashSet::new());

        let rel = FileDependencyRelation::new(&map);

        // 文件2和4是meta文件
        let mut metas = HashSet::new();
        metas.insert(FileId::new(2));
        metas.insert(FileId::new(4));

        let result = rel.get_best_analysis_order(
            &[
                FileId::new(1),
                FileId::new(2),
                FileId::new(3),
                FileId::new(4),
            ],
            &metas,
        );

        // meta文件应该在前面（2和4），非meta文件在后面（1和3）
        assert!(metas.contains(&result[0]), "第一个应该是meta文件");
        assert!(metas.contains(&result[1]), "第二个应该是meta文件");
        assert!(!metas.contains(&result[2]), "第三个应该是非meta文件");
        assert!(!metas.contains(&result[3]), "第四个应该是非meta文件");
    }

    #[test]
    fn test_meta_with_dependencies() {
        let mut map = HashMap::new();
        // File 1 depends on file 2 (meta)
        map.insert(FileId::new(1), {
            let mut s = HashSet::new();
            s.insert(FileId::new(2));
            s
        });
        // File 2 (meta) has no dependencies
        map.insert(FileId::new(2), HashSet::new());
        // File 3 has no dependencies
        map.insert(FileId::new(3), HashSet::new());

        let rel = FileDependencyRelation::new(&map);

        let mut metas = HashSet::new();
        metas.insert(FileId::new(2));

        let result =
            rel.get_best_analysis_order(&[FileId::new(1), FileId::new(2), FileId::new(3)], &metas);

        // File 2 is meta and has no dependencies, should be first
        // File 3 has no dependencies but is not meta, should be second
        // File 1 depends on file 2, should be last
        assert_eq!(result[0], FileId::new(2), "meta file should be first");
        assert_eq!(
            result[1],
            FileId::new(3),
            "non-meta file with no dependencies should be second"
        );
        assert_eq!(
            result[2],
            FileId::new(1),
            "file with dependencies should be last"
        );
    }
}
