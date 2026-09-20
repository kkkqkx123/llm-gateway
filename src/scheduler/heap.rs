use std::cmp::Ordering;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct HeapItem {
    pub id: String,
    pub next: Instant,
    pub index: usize,
}

impl HeapItem {
    pub fn new(id: String, next: Instant) -> Self {
        HeapItem { id, next, index: 0 }
    }
}

impl PartialEq for HeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.next == other.next
    }
}

impl Eq for HeapItem {}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.next.cmp(&other.next)
    }
}

#[derive(Debug, Default)]
pub struct RefreshHeap {
    pub items: Vec<HeapItem>,
}

impl RefreshHeap {
    pub fn new() -> Self {
        RefreshHeap { items: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn peek(&self) -> Option<&HeapItem> {
        self.items.first()
    }

    pub fn push(&mut self, mut item: HeapItem) {
        item.index = self.items.len();
        self.items.push(item);
        self.sift_up(self.items.len() - 1);
    }

    pub fn pop(&mut self) -> Option<HeapItem> {
        if self.items.is_empty() {
            return None;
        }

        let len = self.items.len();
        if len == 1 {
            return self.items.pop();
        }

        let result = self.items.swap_remove(0);
        self.sift_down(0);
        Some(result)
    }

    pub fn remove(&mut self, index: usize) -> Option<HeapItem> {
        if index >= self.items.len() {
            return None;
        }

        let len = self.items.len();
        if index == len - 1 {
            return self.items.pop();
        }

        let item = self.items.swap_remove(index);
        self.sift_down(index);
        self.sift_up(index);
        Some(item)
    }

    pub fn fix(&mut self, index: usize) {
        let old_next = self.items[index].next;
        self.sift_down(index);
        if self.items[index].next == old_next {
            self.sift_up(index);
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    fn sift_up(&mut self, mut idx: usize) {
        while idx > 0 {
            let parent = (idx - 1) / 2;
            if self.items[idx] >= self.items[parent] {
                break;
            }
            self.items.swap(idx, parent);
            self.update_indices(idx, parent);
            idx = parent;
        }
    }

    fn sift_down(&mut self, mut idx: usize) {
        let len = self.items.len();
        loop {
            let left = 2 * idx + 1;
            let right = 2 * idx + 2;
            let mut smallest = idx;

            if left < len && self.items[left] < self.items[smallest] {
                smallest = left;
            }
            if right < len && self.items[right] < self.items[smallest] {
                smallest = right;
            }

            if smallest == idx {
                break;
            }

            self.items.swap(idx, smallest);
            self.update_indices(idx, smallest);
            idx = smallest;
        }
    }

    fn update_indices(&mut self, i: usize, j: usize) {
        self.items[i].index = i;
        self.items[j].index = j;
    }

    pub fn iter(&self) -> impl Iterator<Item = &HeapItem> {
        self.items.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_item_creation() {
        let item = HeapItem::new("test-id".to_string(), Instant::now());
        assert_eq!(item.id, "test-id");
        assert_eq!(item.index, 0);
    }

    #[test]
    fn test_heap_item_ordering() {
        let now = Instant::now();
        let item1 = HeapItem::new("id1".to_string(), now);
        let item2 = HeapItem::new("id2".to_string(), now + Duration::from_secs(1));

        assert_eq!(item1.cmp(&item2), Ordering::Less);
        assert_eq!(item2.cmp(&item1), Ordering::Greater);
        assert_eq!(item1.cmp(&item1), Ordering::Equal);
    }

    #[test]
    fn test_heap_creation() {
        let heap = RefreshHeap::new();
        assert!(heap.is_empty());
        assert_eq!(heap.len(), 0);
    }

    #[test]
    fn test_heap_default() {
        let heap = RefreshHeap::default();
        assert!(heap.is_empty());
    }

    #[test]
    fn test_heap_push_and_len() {
        let mut heap = RefreshHeap::new();

        heap.push(HeapItem::new(
            "id1".to_string(),
            Instant::now() + Duration::from_secs(10),
        ));
        assert_eq!(heap.len(), 1);

        heap.push(HeapItem::new(
            "id2".to_string(),
            Instant::now() + Duration::from_secs(5),
        ));
        assert_eq!(heap.len(), 2);

        heap.push(HeapItem::new(
            "id3".to_string(),
            Instant::now() + Duration::from_secs(15),
        ));
        assert_eq!(heap.len(), 3);
    }

    #[test]
    fn test_heap_peek() {
        let mut heap = RefreshHeap::new();
        assert!(heap.peek().is_none());

        let now = Instant::now();
        heap.push(HeapItem::new(
            "id1".to_string(),
            now + Duration::from_secs(10),
        ));
        heap.push(HeapItem::new(
            "id2".to_string(),
            now + Duration::from_secs(5),
        ));

        let peeked = heap.peek().unwrap();
        assert_eq!(peeked.id, "id2");
    }

    #[test]
    fn test_heap_pop() {
        let mut heap = RefreshHeap::new();
        let now = Instant::now();

        heap.push(HeapItem::new(
            "id1".to_string(),
            now + Duration::from_secs(10),
        ));
        heap.push(HeapItem::new(
            "id2".to_string(),
            now + Duration::from_secs(5),
        ));
        heap.push(HeapItem::new(
            "id3".to_string(),
            now + Duration::from_secs(15),
        ));

        let first = heap.pop().unwrap();
        assert_eq!(first.id, "id2");

        let second = heap.pop().unwrap();
        assert_eq!(second.id, "id1");

        let third = heap.pop().unwrap();
        assert_eq!(third.id, "id3");

        assert!(heap.pop().is_none());
    }

    #[test]
    fn test_heap_pop_empty() {
        let mut heap = RefreshHeap::new();
        assert!(heap.pop().is_none());
    }

    #[test]
    fn test_heap_remove() {
        let mut heap = RefreshHeap::new();
        let now = Instant::now();

        heap.push(HeapItem::new(
            "id1".to_string(),
            now + Duration::from_secs(10),
        ));
        heap.push(HeapItem::new(
            "id2".to_string(),
            now + Duration::from_secs(5),
        ));
        heap.push(HeapItem::new(
            "id3".to_string(),
            now + Duration::from_secs(15),
        ));

        let removed = heap.remove(1).unwrap();
        assert_eq!(removed.id, "id1");
        assert_eq!(heap.len(), 2);

        let removed = heap.remove(0).unwrap();
        assert_eq!(removed.id, "id2");
        assert_eq!(heap.len(), 1);
    }

    #[test]
    fn test_heap_remove_last() {
        let mut heap = RefreshHeap::new();
        let now = Instant::now();

        heap.push(HeapItem::new(
            "id1".to_string(),
            now + Duration::from_secs(10),
        ));
        heap.push(HeapItem::new(
            "id2".to_string(),
            now + Duration::from_secs(5),
        ));

        let removed = heap.remove(1).unwrap();
        assert_eq!(removed.id, "id1");
        assert_eq!(heap.len(), 1);
    }

    #[test]
    fn test_heap_remove_invalid() {
        let mut heap = RefreshHeap::new();
        heap.push(HeapItem::new("id1".to_string(), Instant::now()));

        assert!(heap.remove(5).is_none());
        assert_eq!(heap.len(), 1);
    }

    #[test]
    fn test_heap_fix() {
        let mut heap = RefreshHeap::new();
        let now = Instant::now();

        heap.push(HeapItem::new(
            "id1".to_string(),
            now + Duration::from_secs(10),
        ));
        heap.push(HeapItem::new(
            "id2".to_string(),
            now + Duration::from_secs(5),
        ));
        heap.push(HeapItem::new(
            "id3".to_string(),
            now + Duration::from_secs(15),
        ));

        let peeked_before = heap.peek().unwrap();
        assert_eq!(peeked_before.id, "id2");

        heap.fix(1);

        let peeked_after = heap.peek().unwrap();
        assert_eq!(peeked_after.id, "id2");
    }

    #[test]
    fn test_heap_clear() {
        let mut heap = RefreshHeap::new();

        heap.push(HeapItem::new("id1".to_string(), Instant::now()));
        heap.push(HeapItem::new("id2".to_string(), Instant::now()));

        heap.clear();
        assert!(heap.is_empty());
        assert_eq!(heap.len(), 0);
    }

    #[test]
    fn test_heap_iter() {
        let mut heap = RefreshHeap::new();
        let now = Instant::now();

        heap.push(HeapItem::new(
            "id1".to_string(),
            now + Duration::from_secs(10),
        ));
        heap.push(HeapItem::new(
            "id2".to_string(),
            now + Duration::from_secs(5),
        ));
        heap.push(HeapItem::new(
            "id3".to_string(),
            now + Duration::from_secs(15),
        ));

        let ids: Vec<&str> = heap.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"id1"));
        assert!(ids.contains(&"id2"));
        assert!(ids.contains(&"id3"));
    }

    #[test]
    fn test_heap_min_heap_property() {
        let mut heap = RefreshHeap::new();
        let now = Instant::now();

        heap.push(HeapItem::new(
            "id5".to_string(),
            now + Duration::from_secs(50),
        ));
        heap.push(HeapItem::new(
            "id2".to_string(),
            now + Duration::from_secs(20),
        ));
        heap.push(HeapItem::new(
            "id3".to_string(),
            now + Duration::from_secs(30),
        ));
        heap.push(HeapItem::new(
            "id1".to_string(),
            now + Duration::from_secs(10),
        ));
        heap.push(HeapItem::new(
            "id4".to_string(),
            now + Duration::from_secs(40),
        ));

        let mut prev_time = None;
        while let Some(item) = heap.pop() {
            if let Some(prev) = prev_time {
                assert!(item.next >= prev);
            }
            prev_time = Some(item.next);
        }
    }
}
