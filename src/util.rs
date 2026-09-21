use std::collections::BTreeSet;

pub trait FindLowest {
    fn find_lowest(&self) -> u32;
}

impl FindLowest for BTreeSet<u32> {
    fn find_lowest(&self) -> u32 {
        let mut candidate = 1;

        for &used in self {
            if used > candidate {
                return candidate;
            }
            candidate = used.saturating_add(1);
        }

        if candidate == 0 { 1 } else { candidate }
    }
}

#[cfg(test)]
mod refnum_tests {
    use super::*;

    #[test]
    fn test_empty_set() {
        let set = BTreeSet::new();
        assert_eq!(set.find_lowest(), 1);
    }

    #[test]
    fn test_no_gaps() {
        let set: BTreeSet<_> = [1, 2, 3].into_iter().collect();
        assert_eq!(set.find_lowest(), 4);
    }

    #[test]
    fn test_overflow_handling() {
        let set: BTreeSet<_> = [u32::MAX, 1, 2].into_iter().collect();
        assert_eq!(set.find_lowest(), 3); // Wraps but skips 0.
    }
}

#[cfg(test)]
mod bench_large_sets {
    use super::*;
    use std::time::Instant;

    #[test]
    fn bench_find_lowest_in_large_set() {
        // Large set (1 million elements, no gaps)
        let large_set: BTreeSet<u32> = (1..=1_000_000).collect();

        let start = Instant::now();
        let result = large_set.find_lowest();
        let duration = start.elapsed();

        println!("[LARGE SET] Result: {result}, Time taken: {duration:?}");
    }

    #[test]
    fn bench_find_lowest_in_large_set_with_gap() {
        let max = 10_000_000u32;
        let mut large_set: BTreeSet<u32> = (1..=max).collect();

        large_set.remove(&(max - 1));

        let start = Instant::now();
        let result = large_set.find_lowest();
        let duration = start.elapsed();

        println!("[LARGE SET WITH GAP] Result: {result}, Time taken: {duration:?}");
    }
}

#[cfg(test)]
mod bench_small_sets {
    use super::*;
    use std::time::Instant;

    #[test]
    fn bench_find_lowest_in_small_set() {
        let small_set: BTreeSet<u32> = (1..=10).collect();

        let start = Instant::now();
        let result = small_set.find_lowest();
        let duration = start.elapsed();

        println!("[SMALL SET] Result: {result}, Time taken: {duration:?}");
    }

    #[test]
    fn bench_find_lowest_in_small_set_with_gap() {
        let small_set: BTreeSet<u32> = [1, 3, 4].into_iter().collect();

        let start = Instant::now();
        let result = small_set.find_lowest();
        let duration = start.elapsed();

        println!("[SMALL SET WITH GAP] Result: {result}, Time taken: {duration:?}");
    }
}
