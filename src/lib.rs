#![feature(plugin)]
#![plugin(clippy)]

//! Threadsafe garbage collector (the `Orc<T>` type).
//!
//! The Orc<T> type provides shared ownership over an immutable value that is
//! in stored in a preallocated memory area.
//! As soon as the last reference to a stored value is gone the value is dropped.
//! In addition to that cycles are reclaimed if the space is needed for
//! new allocations.
//!
//! While there may be some useful applications in pure rust programms for this
//! of memory managment scheme, the intended use case is garbage collection for
//! unityped (speak: dynamic) languages written in and tightly integrated with
//! rust.
//!

use std::mem::{transmute, size_of, transmute_copy, forget};
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
	use std::marker::PhantomData;
use std::marker::Sync;
use std::cell::Cell;

// constants

// change to const PTR_SIZE: usize = size_of::<usize>() as soon it's a const fn
#[cfg(target_pointer_width = "32")]
const PTR_SIZE: usize = 4;
#[cfg(target_pointer_width = "64")]
const PTR_SIZE: usize = 8;

const MAX_WEIGHT_EXP: u8 = PTR_SIZE as u8 * 8 - 1;
const MAX_WEIGHT: usize = 1usize << MAX_WEIGHT_EXP; // 2^MAX_WEIGHT_EXP

/// A pointer into an OrcHeap. Can be shared across threads.
pub struct Orc<'a, T: 'a> {
    pointer_data: [u8; PTR_SIZE - 1], // the ptr is in little endian byteorder
    weight_exp: Cell<u8>,
    lifetime_and_type: PhantomData<&'a T>,
}

unsafe impl<'a, T> Sync for Orc<'a, T> {}

impl<'a, T> Drop for Orc<'a, T> {
    fn drop(&mut self) {
        let slot = construct_pointer::<T>(self.pointer_data, 0);
        let weight = two_two_the(self.weight_exp.get());
        slot.weight.fetch_sub(weight, Ordering::Release);
    }
}

impl<'a, T> Clone for Orc<'a, T> {
    fn clone(&self) -> Orc<'a, T> {
        if self.weight_exp.get() > 1 {
            self.weight_exp.set(self.weight_exp.get() - 1);
            return Orc {
                weight_exp: Cell::new(self.weight_exp.get()),
                pointer_data: self.pointer_data,
                lifetime_and_type: PhantomData,
            };
        }
        panic!("not implemented yet");
    }
}

impl<'a, T> Deref for Orc<'a, T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        let slot = construct_pointer::<T>(self.pointer_data, 0);
        match slot.data {
            Some(ref d) => d,
            None => unreachable!(), // since a reference is in existence
        }
    }
}

// wrapper around the type T, that is saved in the heap
//
struct OrcInner<T> {
    weight: AtomicUsize,
    data: Option<T>,
}

// The heap that holds all allocated values
pub struct OrcHeap<T> {
    heap: Vec<OrcInner<T>>,
}

unsafe impl<'a, T> Sync for OrcHeap<T> {}

impl<'a, T> OrcHeap<T> {
    /// Creates a new Heap of sensible size (for certain definitions of sensible)
    /// # Example:
    /// ```
    /// use orc::OrcHeap;
    /// let heap = OrcHeap::<usize>::new();
    /// ```
    pub fn new() -> OrcHeap<T> {
        const DEFAULT_HEAP_SIZE: usize = 16;
        OrcHeap::<T>::with_capacity(DEFAULT_HEAP_SIZE)
    }

    /// Creates a new Heap of a user defined size
    /// # Example:
    /// ```
    /// use orc::OrcHeap;
    /// let heap = OrcHeap::<usize>::with_capacity(42);
    /// ```
    pub fn with_capacity(capacity: usize) -> OrcHeap<T> {
        let mut heap = Vec::with_capacity(capacity);
        // it is important that no other push operations on any of theses vectors are performed
        for _ in 0..capacity {
            heap.push(OrcInner {
                weight: AtomicUsize::new(0),
                data: None,
            });
        }
        // make sure that all pointers have enough headroom to store the weight
        let (_, weight) = deconstruct_pointer(heap.iter().nth(capacity - 1).unwrap());
        assert_eq!(weight, 0);

        OrcHeap::<T> { heap: heap }
    }


    /// Allocates a Value in the heap.
    pub fn alloc(&'a self, value: T) -> Result<Orc<T>, &'static str> {
        // find an empty slot

        let mut position = 0;
        loop {
            unsafe {
                let slot = self.heap.get_unchecked(position);
                if slot.weight.compare_and_swap(0, MAX_WEIGHT, Ordering::Relaxed) == 0 {
                    // a little dance to make the gods of borrow checking happy
                    let ref data: Option<T> = slot.data;
                    let mut_data: *mut Option<T> = hack_transmute(data);
                    // overwrite the data
                    *mut_data = Some(value);
                    // give out the pointer
                    let (pointer_data, _) = deconstruct_pointer(slot);
                    return Ok(Orc::<'a, T> {
                        pointer_data: pointer_data,
                        weight_exp: Cell::new(MAX_WEIGHT_EXP),
                        lifetime_and_type: PhantomData,
                    });
                }
            }

            position += 1;
            if position == self.heap.capacity() {
                position = 0;
                // Just for now
                break;
            }
        }
        Err("Out of memory")
    }


    pub fn collect(&'a self) {
        for position in 0..self.heap.capacity() {
            unsafe {
                let slot = self.heap.get_unchecked(position);
                if slot.weight.compare_and_swap(0, MAX_WEIGHT, Ordering::Relaxed) == 0 {
                    let ref data: Option<T> = slot.data;
                    let mut_data: *mut Option<T> = hack_transmute(data);
                    // overwrite the data
                    *mut_data = None;
                }
            }
        }
    }
}


// helper functions
//
#[inline(always)]
fn deconstruct_pointer<T>(p: &OrcInner<T>) -> ([u8; PTR_SIZE - 1], u8) {
    unsafe {
        let p: usize = transmute(p);
        transmute(usize::from_le(p)) // NOOP on little endian machines
    }
}

#[inline(always)]
fn construct_pointer<'a, T>(pointer: [u8; PTR_SIZE - 1], weight: u8) -> &'a OrcInner<T> {
    unsafe {
        let p: usize = transmute((pointer, weight));
        transmute(usize::from_le(p)) // NOOP on little endian machines
    }
}

#[inline(always)]
fn two_two_the(exp: u8) -> usize {
    1usize << exp
}

// use this instead of transmute to work around [E0139]
#[inline(always)]
unsafe fn hack_transmute<T, U>(x: T) -> U {
    debug_assert_eq!(size_of::<T>(), size_of::<U>());
    let y = transmute_copy(&x);
    forget(x);
    y
}

// unit tests
//
#[test]
fn test_two_two_the() {
    assert_eq!(two_two_the(0), 1);
    assert_eq!(two_two_the(1), 2);
    assert_eq!(two_two_the(8), 256);
}


// functional test
//
#[cfg(test)]
mod test_drop {
    use OrcHeap;
    use std::cell::Cell;

    struct DropTest<'a>(&'a Cell<usize>);

    impl<'a> Drop for DropTest<'a> {
        fn drop(&mut self) {
            let v = self.0.get();
            self.0.set(v - 1);
        }
    }

    #[test]
    #[allow(unused_variables)]
    fn test_drop() {
        let test_size = 1000;
        let values_in_existence = Cell::new(test_size);

        let heap = OrcHeap::with_capacity(test_size);

        for _ in 0..test_size {
            let o = heap.alloc(DropTest(&values_in_existence)).unwrap();
        }
        heap.collect();
        assert_eq!(values_in_existence.get(), 0);
    }

    #[test]
    #[allow(unused_variables)]
    fn test_heap_freed() {
        let test_size = 2;
        let values_in_existence = Cell::new(5);

        let heap = OrcHeap::with_capacity(test_size);

        {
            let a = heap.alloc(DropTest(&values_in_existence)).unwrap();
            let b = heap.alloc(DropTest(&values_in_existence)).unwrap();
        }
        // now the heap should be freed and the allocations should be possible
        let c = heap.alloc(DropTest(&values_in_existence)).unwrap();
        let d = heap.alloc(DropTest(&values_in_existence)).unwrap();
        assert_eq!(values_in_existence.get(), 3); // a and b are dropped

        // and this must fail
        assert!(heap.alloc(DropTest(&values_in_existence)).is_err())
    }
}

#[cfg(test)]
mod test_concurrency {
    // this test may not fail, even if something is wrong with the concurrent
    // allocation behaviour. But with a high enough test_size, it will most
    // likely blow up.
    extern crate crossbeam;
    use OrcHeap;

    #[test]
    fn test_concurrency() {
        extern crate crossbeam;
        let test_size = 1000;

        let heap = OrcHeap::with_capacity(test_size * 10);

        crossbeam::scope(|scope| {
            for _ in 0..test_size {
                scope.spawn(|| {
                    for j in 0..test_size {
                        if let Ok(v) = heap.alloc(j) {
                            assert_eq!(*v, j);
                        }
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod test_cycle_collection {

    #[test]
    fn test_concurrency() {
        extern crate crossbeam;
        let test_size = 1000;

        let heap = OrcHeap::with_capacity(test_size * 10);

        crossbeam::scope(|scope| {
            for _ in 0..test_size {
                scope.spawn(|| {
                    for j in 0..test_size {
                        if let Ok(v) = heap.alloc(j) {
                            assert_eq!(*v, j);
                        }
                    }
                });
            }
        });
    }
}
