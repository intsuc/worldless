use std::cmp::Ordering;

const DEFAULT_CAPACITY: usize = 16;
const DEFAULT_THRESHOLD: usize = 12;
const TREEIFY_THRESHOLD: usize = 8;
const UNTREEIFY_THRESHOLD: usize = 6;
const MIN_TREEIFY_CAPACITY: usize = 64;

pub(crate) fn java_hash_set_order<T>(
    input: impl IntoIterator<Item = T>,
    hash_code: impl Fn(&T) -> i32,
    equals: impl Fn(&T, &T) -> bool,
    comparable_order: impl Fn(&T, &T) -> Option<Ordering>,
    tie_break_order: impl Fn(&T, &T) -> Ordering,
) -> Vec<T> {
    let mut set = JavaHashSet::new(&equals, &comparable_order, &tie_break_order);
    for value in input {
        let hash = spread_hash(hash_code(&value));
        set.insert(value, hash);
    }
    set.into_iter_order()
}

pub(crate) fn java_utf16_hash_code(input: impl IntoIterator<Item = u16>) -> i32 {
    input.into_iter().fold(0_i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    })
}

fn spread_hash(hash: i32) -> i32 {
    let bits = hash as u32;
    (bits ^ (bits >> 16)) as i32
}

struct Node<T> {
    value: Option<T>,
    hash: i32,
    next: Option<usize>,
    previous: Option<usize>,
    parent: Option<usize>,
    left: Option<usize>,
    right: Option<usize>,
    red: bool,
    tree: bool,
}

impl<T> Node<T> {
    fn list(value: T, hash: i32) -> Self {
        Self {
            value: Some(value),
            hash,
            next: None,
            previous: None,
            parent: None,
            left: None,
            right: None,
            red: false,
            tree: false,
        }
    }
}

struct JavaHashSet<'a, T, E, C, O> {
    nodes: Vec<Node<T>>,
    table: Vec<Option<usize>>,
    size: usize,
    threshold: usize,
    equals: &'a E,
    comparable_order: &'a C,
    tie_break_order: &'a O,
}

impl<'a, T, E, C, O> JavaHashSet<'a, T, E, C, O>
where
    E: Fn(&T, &T) -> bool,
    C: Fn(&T, &T) -> Option<Ordering>,
    O: Fn(&T, &T) -> Ordering,
{
    fn new(equals: &'a E, comparable_order: &'a C, tie_break_order: &'a O) -> Self {
        Self {
            nodes: Vec::new(),
            table: Vec::new(),
            size: 0,
            threshold: 0,
            equals,
            comparable_order,
            tie_break_order,
        }
    }

    fn insert(&mut self, value: T, hash: i32) {
        if self.table.is_empty() {
            self.resize();
        }
        let bucket = bucket_index(hash, self.table.len());
        let Some(first) = self.table[bucket] else {
            let node = self.push_node(value, hash);
            self.table[bucket] = Some(node);
            self.finish_insertion();
            return;
        };

        if self.nodes[first].tree {
            if self.insert_tree_node(bucket, first, value, hash) {
                self.finish_insertion();
            }
            return;
        }

        let mut current = first;
        let mut bin_count = 0;
        loop {
            if self.node_matches(current, &value, hash) {
                return;
            }
            let Some(next) = self.nodes[current].next else {
                let node = self.push_node(value, hash);
                self.nodes[current].next = Some(node);
                if bin_count >= TREEIFY_THRESHOLD - 1 {
                    self.treeify_bin(hash);
                }
                self.finish_insertion();
                return;
            };
            current = next;
            bin_count += 1;
        }
    }

    fn node_matches(&self, node: usize, value: &T, hash: i32) -> bool {
        self.nodes[node].hash == hash
            && (self.equals)(value, self.nodes[node].value.as_ref().expect("live node"))
    }

    fn push_node(&mut self, value: T, hash: i32) -> usize {
        let index = self.nodes.len();
        self.nodes.push(Node::list(value, hash));
        index
    }

    fn finish_insertion(&mut self) {
        self.size += 1;
        if self.size > self.threshold {
            self.resize();
        }
    }

    fn treeify_bin(&mut self, hash: i32) {
        if self.table.len() < MIN_TREEIFY_CAPACITY {
            self.resize();
            return;
        }
        let bucket = bucket_index(hash, self.table.len());
        let Some(head) = self.table[bucket] else {
            return;
        };
        let mut previous = None;
        let mut current = Some(head);
        while let Some(node) = current {
            self.nodes[node].tree = true;
            self.nodes[node].previous = previous;
            self.clear_tree_links(node);
            previous = Some(node);
            current = self.nodes[node].next;
        }
        self.treeify(head, bucket);
    }

    fn treeify(&mut self, head: usize, bucket: usize) {
        let mut root = None;
        let mut current = Some(head);
        while let Some(node) = current {
            let next = self.nodes[node].next;
            self.nodes[node].left = None;
            self.nodes[node].right = None;
            if let Some(mut parent) = root {
                loop {
                    let direction = self.tree_direction(node, parent);
                    let child = if direction == Ordering::Greater {
                        self.nodes[parent].right
                    } else {
                        self.nodes[parent].left
                    };
                    if let Some(child) = child {
                        parent = child;
                    } else {
                        self.nodes[node].parent = Some(parent);
                        if direction == Ordering::Greater {
                            self.nodes[parent].right = Some(node);
                        } else {
                            self.nodes[parent].left = Some(node);
                        }
                        root = Some(self.balance_insertion(root.expect("tree root"), node));
                        break;
                    }
                }
            } else {
                self.nodes[node].parent = None;
                self.nodes[node].red = false;
                root = Some(node);
            }
            current = next;
        }
        self.move_root_to_front(bucket, root.expect("treeified bin is nonempty"));
    }

    fn tree_direction(&self, node: usize, parent: usize) -> Ordering {
        match self.nodes[node].hash.cmp(&self.nodes[parent].hash) {
            Ordering::Equal => {
                let value = self.nodes[node].value.as_ref().expect("live node");
                let parent_value = self.nodes[parent].value.as_ref().expect("live node");
                (self.comparable_order)(value, parent_value)
                    .filter(|ordering| *ordering != Ordering::Equal)
                    .unwrap_or_else(|| self.value_tie_break(node, parent))
            }
            ordering => ordering,
        }
    }

    fn value_tie_break(&self, node: usize, parent: usize) -> Ordering {
        let ordering = (self.tie_break_order)(
            self.nodes[node].value.as_ref().expect("live node"),
            self.nodes[parent].value.as_ref().expect("live node"),
        );
        assert_ne!(
            ordering,
            Ordering::Equal,
            "tie_break_order must distinguish unequal values with equal spread hashes"
        );
        ordering
    }

    fn insert_tree_node(&mut self, bucket: usize, first: usize, value: T, hash: i32) -> bool {
        let mut root = self.root(first);
        let mut parent = root;
        let mut searched = false;
        loop {
            let direction = match hash.cmp(&self.nodes[parent].hash) {
                Ordering::Equal if self.node_matches(parent, &value, hash) => return false,
                Ordering::Equal => match (self.comparable_order)(
                    &value,
                    self.nodes[parent].value.as_ref().expect("live node"),
                ) {
                    Some(ordering) if ordering != Ordering::Equal => ordering,
                    _ => {
                        if !searched {
                            searched = true;
                            let found_left = self.nodes[parent]
                                .left
                                .and_then(|child| self.find_tree_node(child, &value, hash));
                            let found = found_left.or_else(|| {
                                self.nodes[parent]
                                    .right
                                    .and_then(|child| self.find_tree_node(child, &value, hash))
                            });
                            if found.is_some() {
                                return false;
                            }
                        }
                        let ordering = (self.tie_break_order)(
                            &value,
                            self.nodes[parent].value.as_ref().expect("live node"),
                        );
                        assert_ne!(
                            ordering,
                            Ordering::Equal,
                            "tie_break_order must distinguish unequal values with equal spread hashes"
                        );
                        ordering
                    }
                },
                ordering => ordering,
            };
            let child = if direction == Ordering::Greater {
                self.nodes[parent].right
            } else {
                self.nodes[parent].left
            };
            if let Some(child) = child {
                parent = child;
                continue;
            }

            let following = self.nodes[parent].next;
            let node = self.push_node(value, hash);
            self.nodes[node].tree = true;
            self.nodes[node].parent = Some(parent);
            self.nodes[node].previous = Some(parent);
            self.nodes[node].next = following;
            self.nodes[parent].next = Some(node);
            if let Some(following) = following {
                self.nodes[following].previous = Some(node);
            }
            if direction == Ordering::Greater {
                self.nodes[parent].right = Some(node);
            } else {
                self.nodes[parent].left = Some(node);
            }
            root = self.balance_insertion(root, node);
            self.move_root_to_front(bucket, root);
            return true;
        }
    }

    fn find_tree_node(&self, node: usize, value: &T, hash: i32) -> Option<usize> {
        let node_hash = self.nodes[node].hash;
        if node_hash > hash {
            return self.nodes[node]
                .left
                .and_then(|child| self.find_tree_node(child, value, hash));
        }
        if node_hash < hash {
            return self.nodes[node]
                .right
                .and_then(|child| self.find_tree_node(child, value, hash));
        }
        if self.node_matches(node, value, hash) {
            return Some(node);
        }
        match (self.nodes[node].left, self.nodes[node].right) {
            (None, right) => right.and_then(|child| self.find_tree_node(child, value, hash)),
            (left, None) => left.and_then(|child| self.find_tree_node(child, value, hash)),
            (Some(left), Some(right)) => {
                if let Some(ordering) = (self.comparable_order)(
                    value,
                    self.nodes[node].value.as_ref().expect("live node"),
                ) && ordering != Ordering::Equal
                {
                    return self.find_tree_node(
                        if ordering == Ordering::Less {
                            left
                        } else {
                            right
                        },
                        value,
                        hash,
                    );
                }
                self.find_tree_node(right, value, hash)
                    .or_else(|| self.find_tree_node(left, value, hash))
            }
        }
    }

    fn root(&self, mut node: usize) -> usize {
        while let Some(parent) = self.nodes[node].parent {
            node = parent;
        }
        node
    }

    fn move_root_to_front(&mut self, bucket: usize, root: usize) {
        let first = self.table[bucket].expect("tree bin has a head");
        if first == root {
            return;
        }
        let previous = self.nodes[root].previous;
        let next = self.nodes[root].next;
        if let Some(next) = next {
            self.nodes[next].previous = previous;
        }
        if let Some(previous) = previous {
            self.nodes[previous].next = next;
        }
        self.nodes[first].previous = Some(root);
        self.nodes[root].next = Some(first);
        self.nodes[root].previous = None;
        self.table[bucket] = Some(root);
    }

    fn balance_insertion(&mut self, mut root: usize, mut node: usize) -> usize {
        self.nodes[node].red = true;
        loop {
            let Some(parent) = self.nodes[node].parent else {
                self.nodes[node].red = false;
                return node;
            };
            if !self.nodes[parent].red {
                return root;
            }
            let Some(grandparent) = self.nodes[parent].parent else {
                return root;
            };
            if self.nodes[grandparent].left == Some(parent) {
                let uncle = self.nodes[grandparent].right;
                if uncle.is_some_and(|uncle| self.nodes[uncle].red) {
                    self.nodes[uncle.expect("checked")].red = false;
                    self.nodes[parent].red = false;
                    self.nodes[grandparent].red = true;
                    node = grandparent;
                    continue;
                }
                let mut parent = parent;
                let mut grandparent = grandparent;
                if self.nodes[parent].right == Some(node) {
                    root = self.rotate_left(root, parent);
                    node = parent;
                    parent = self.nodes[node].parent.expect("rotation parent");
                    grandparent = self.nodes[parent].parent.expect("rotation grandparent");
                }
                self.nodes[parent].red = false;
                self.nodes[grandparent].red = true;
                return self.rotate_right(root, grandparent);
            }

            let uncle = self.nodes[grandparent].left;
            if uncle.is_some_and(|uncle| self.nodes[uncle].red) {
                self.nodes[uncle.expect("checked")].red = false;
                self.nodes[parent].red = false;
                self.nodes[grandparent].red = true;
                node = grandparent;
                continue;
            }
            let mut parent = parent;
            let mut grandparent = grandparent;
            if self.nodes[parent].left == Some(node) {
                root = self.rotate_right(root, parent);
                node = parent;
                parent = self.nodes[node].parent.expect("rotation parent");
                grandparent = self.nodes[parent].parent.expect("rotation grandparent");
            }
            self.nodes[parent].red = false;
            self.nodes[grandparent].red = true;
            return self.rotate_left(root, grandparent);
        }
    }

    fn rotate_left(&mut self, mut root: usize, node: usize) -> usize {
        let Some(right) = self.nodes[node].right else {
            return root;
        };
        let right_left = self.nodes[right].left;
        self.nodes[node].right = right_left;
        if let Some(right_left) = right_left {
            self.nodes[right_left].parent = Some(node);
        }
        let parent = self.nodes[node].parent;
        self.nodes[right].parent = parent;
        if let Some(parent) = parent {
            if self.nodes[parent].left == Some(node) {
                self.nodes[parent].left = Some(right);
            } else {
                self.nodes[parent].right = Some(right);
            }
        } else {
            self.nodes[right].red = false;
            root = right;
        }
        self.nodes[right].left = Some(node);
        self.nodes[node].parent = Some(right);
        root
    }

    fn rotate_right(&mut self, mut root: usize, node: usize) -> usize {
        let Some(left) = self.nodes[node].left else {
            return root;
        };
        let left_right = self.nodes[left].right;
        self.nodes[node].left = left_right;
        if let Some(left_right) = left_right {
            self.nodes[left_right].parent = Some(node);
        }
        let parent = self.nodes[node].parent;
        self.nodes[left].parent = parent;
        if let Some(parent) = parent {
            if self.nodes[parent].right == Some(node) {
                self.nodes[parent].right = Some(left);
            } else {
                self.nodes[parent].left = Some(left);
            }
        } else {
            self.nodes[left].red = false;
            root = left;
        }
        self.nodes[left].right = Some(node);
        self.nodes[node].parent = Some(left);
        root
    }

    fn resize(&mut self) {
        let old_capacity = self.table.len();
        let new_capacity = if old_capacity == 0 {
            DEFAULT_CAPACITY
        } else {
            old_capacity
                .checked_mul(2)
                .expect("Java HashSet capacity overflow")
        };
        self.threshold = if old_capacity == 0 {
            DEFAULT_THRESHOLD
        } else {
            self.threshold
                .checked_mul(2)
                .expect("Java HashSet threshold overflow")
        };
        let old_table = std::mem::replace(&mut self.table, vec![None; new_capacity]);
        for (bucket, head) in old_table.into_iter().enumerate() {
            let Some(head) = head else {
                continue;
            };
            if self.nodes[head].next.is_none() {
                let new_bucket = bucket_index(self.nodes[head].hash, new_capacity);
                self.table[new_bucket] = Some(head);
            } else if self.nodes[head].tree {
                self.split_tree_bin(head, bucket, old_capacity);
            } else {
                self.split_list_bin(head, bucket, old_capacity);
            }
        }
    }

    fn split_list_bin(&mut self, head: usize, bucket: usize, split_bit: usize) {
        let (low, high) = self.partition_chain(head, split_bit, false);
        self.table[bucket] = low.0;
        self.table[bucket + split_bit] = high.0;
    }

    fn split_tree_bin(&mut self, head: usize, bucket: usize, split_bit: usize) {
        let (low, high) = self.partition_chain(head, split_bit, true);
        self.finish_tree_split(bucket, low, high.0.is_some());
        self.finish_tree_split(bucket + split_bit, high, low.0.is_some());
    }

    fn finish_tree_split(
        &mut self,
        bucket: usize,
        (head, count): (Option<usize>, usize),
        other_side_exists: bool,
    ) {
        let Some(head) = head else {
            return;
        };
        self.table[bucket] = Some(head);
        if count <= UNTREEIFY_THRESHOLD {
            let mut current = Some(head);
            while let Some(node) = current {
                self.nodes[node].tree = false;
                self.nodes[node].previous = None;
                self.clear_tree_links(node);
                current = self.nodes[node].next;
            }
        } else if other_side_exists {
            self.treeify(head, bucket);
        }
    }

    fn partition_chain(
        &mut self,
        head: usize,
        split_bit: usize,
        tree: bool,
    ) -> ((Option<usize>, usize), (Option<usize>, usize)) {
        let mut low_head = None;
        let mut low_tail = None;
        let mut low_count = 0;
        let mut high_head = None;
        let mut high_tail = None;
        let mut high_count = 0;
        let mut current = Some(head);
        while let Some(node) = current {
            current = self.nodes[node].next;
            self.nodes[node].next = None;
            let high = (self.nodes[node].hash as u32 as usize & split_bit) != 0;
            let (chain_head, chain_tail, count) = if high {
                (&mut high_head, &mut high_tail, &mut high_count)
            } else {
                (&mut low_head, &mut low_tail, &mut low_count)
            };
            self.nodes[node].previous = tree.then_some(*chain_tail).flatten();
            if let Some(tail) = *chain_tail {
                self.nodes[tail].next = Some(node);
            } else {
                *chain_head = Some(node);
            }
            *chain_tail = Some(node);
            *count += 1;
        }
        ((low_head, low_count), (high_head, high_count))
    }

    fn clear_tree_links(&mut self, node: usize) {
        self.nodes[node].parent = None;
        self.nodes[node].left = None;
        self.nodes[node].right = None;
        self.nodes[node].red = false;
    }

    fn into_iter_order(mut self) -> Vec<T> {
        let mut result = Vec::with_capacity(self.size);
        for head in self.table {
            let mut current = head;
            while let Some(node) = current {
                current = self.nodes[node].next;
                result.push(self.nodes[node].value.take().expect("live node"));
            }
        }
        result
    }
}

fn bucket_index(hash: i32, capacity: usize) -> usize {
    hash as u32 as usize & (capacity - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Key {
        id: i32,
        hash: i32,
        equality: i32,
    }

    fn key(id: i32, hash: i32) -> Key {
        Key {
            id,
            hash,
            equality: id,
        }
    }

    fn order(input: Vec<Key>) -> Vec<i32> {
        java_hash_set_order(
            input,
            |key| key.hash,
            |left, right| left.equality == right.equality,
            |left, right| Some(left.id.cmp(&right.id)),
            |left, right| left.id.cmp(&right.id),
        )
        .into_iter()
        .map(|key| key.id)
        .collect()
    }

    #[test]
    fn deduplicates_with_java_hash_and_equals() {
        let mut duplicate = key(99, 0);
        duplicate.equality = 1;
        assert_eq!(order(vec![key(1, 0), duplicate, key(2, -1)]), [1, 2]);
    }

    #[test]
    fn resize_preserves_each_split_chain() {
        let input = vec![
            key(0, 16),
            key(1, 0),
            key(2, 17),
            key(3, 1),
            key(4, 18),
            key(5, 2),
            key(6, 19),
            key(7, 3),
            key(8, 20),
            key(9, 4),
            key(10, 21),
            key(11, 5),
            key(12, 22),
        ];
        assert_eq!(order(input), [1, 3, 5, 7, 9, 11, 0, 2, 4, 6, 8, 10, 12]);
    }

    #[test]
    fn treeification_moves_the_balanced_root_to_the_bucket_front() {
        let ids = [9, 1, 8, 2, 7, 3, 6, 4, 5, 0, 10];
        assert_eq!(
            order(ids.into_iter().map(|id| key(id, 0)).collect()),
            [6, 9, 1, 8, 2, 7, 3, 4, 5, 0, 10]
        );
    }

    #[test]
    fn tree_insertions_relink_after_the_search_parent() {
        let ids = [9, 1, 8, 2, 7, 3, 6, 4, 5, 0, 10, 12, 11, 15, 14, 13];
        assert_eq!(
            order(ids.into_iter().map(|id| key(id, 0)).collect()),
            [6, 9, 1, 8, 2, 7, 3, 4, 5, 0, 10, 12, 15, 14, 13, 11]
        );
    }

    #[test]
    fn resizing_splits_and_rebuilds_tree_bins() {
        let mut input = Vec::new();
        for id in [9, 1, 8, 2, 7] {
            input.push(key(100 + id, 0));
        }
        for id in [3, 6, 4, 0, 5, 10, 11] {
            input.push(key(100 + id, 64));
        }
        for id in 0..37 {
            input.push(key(id, id + 1));
        }
        let colliders = order(input)
            .into_iter()
            .filter(|id| *id >= 100)
            .collect::<Vec<_>>();
        assert_eq!(
            colliders,
            [109, 101, 108, 102, 107, 104, 103, 106, 100, 105, 110, 111]
        );
    }

    #[test]
    fn non_comparable_tree_uses_the_explicit_identity_tie_break() {
        #[derive(Debug)]
        struct IdentityKey {
            id: i32,
            identity_hash: i32,
        }

        let identity_hashes = [
            1_025_799_482,
            1_908_316_405,
            1_873_653_341,
            25_126_016,
            762_218_386,
            672_320_506,
            718_231_523,
            1_349_414_238,
            157_627_094,
            932_607_259,
            1_740_000_325,
        ];
        let input = identity_hashes
            .into_iter()
            .enumerate()
            .map(|(id, identity_hash)| IdentityKey {
                id: id as i32,
                identity_hash,
            });
        let actual = java_hash_set_order(
            input,
            |_| 0,
            |left, right| left.id == right.id,
            |_, _| None,
            |left, right| {
                if left.identity_hash <= right.identity_hash {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            },
        )
        .into_iter()
        .map(|key| key.id)
        .collect::<Vec<_>>();
        assert_eq!(actual, [4, 0, 1, 2, 3, 5, 6, 7, 8, 9, 10]);
    }
}
