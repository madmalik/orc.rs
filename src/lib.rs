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

use std::mem::transmute;
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

/// A pointer into an OrcHeap. Can be shared across threads.
pub struct Orc<'a, T: 'a> {
    pointer_data: [u8; PTR_SIZE - 1], // the ptr is in little endian byteorder
    weight_exp: Cell<u8>,
    lifetime_and_type: PhantomData<&'a T>,
}

unsafe impl<'a, T> Sync for Orc<'a, T> {}

impl<'a, T> Drop for Orc<'a, T> {
    fn drop(&mut self) {
        if self.weight_exp.get() == MAX_WEIGHT_EXP {
            let slot = construct_pointer_to_mut::<T>(self.pointer_data, 0);
            unsafe { *slot = OrcInner::None }
        } else {
            let slot = construct_pointer_to_mut::<T>(self.pointer_data, 0);
            let weight = two_two_the(self.weight_exp.get());

            unsafe {
                if match *slot {
                    OrcInner::Some {
						weight: ref inner_weight,
						data: _
					} => inner_weight.fetch_sub(weight, Ordering::Release),
                    OrcInner::None => unreachable!(),
                } == weight {
                    *slot = OrcInner::None;
                }
            }
        }
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
        match slot {
            &OrcInner::Some{
        		weight: _,
        		data: ref d
        	} => d,
            &OrcInner::None => unreachable!(),
        }
    }
}

// wrapper around the type T, that is saved in the heap
//
enum OrcInner<T> {
    Some {
        weight: AtomicUsize,
        data: T,
    },
    None,
}

// The heap that holds all allocated values
pub struct OrcHeap<T> {
    heap: Vec<OrcInner<T>>,
}


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
            heap.push(OrcInner::None);
        }
        // make sure that all pointers have enough headroom to store the weight
        let (_, weight) = deconstruct_pointer(heap.iter().nth(capacity - 1).unwrap());
        assert_eq!(weight, 0);

        OrcHeap::<T> { heap: heap }
    }


	/// Allocates a Value in the heap.
	/// # Example:
	/// ```
	/// use orc::OrcHeap;
	/// let heap = OrcHeap::<AtomicUsize>::with_capacity(42);
	/// ```
    pub fn alloc(&'a self, value: T) -> Result<Orc<T>, &'static str> {
        // find an empty slot
        if let Some(position) = (&self.heap).iter().position(|x| {
            match x {
                &OrcInner::None => true,
                _ => false,
            }
        }) {
            unsafe {
                // create a mutable reference to the slot
                let slot: *mut OrcInner<T> = transmute(self.heap.get_unchecked(position));
                // overwrite it. Highly unsafe!
                *slot = OrcInner::Some {
                    weight: AtomicUsize::new(two_two_the(MAX_WEIGHT_EXP)),
                    data: value,
                };

                // extract relevant pointer data
                let (pointer_data, _) = deconstruct_pointer(self.heap.get_unchecked(position));

                // give out the reference with max weight
                return Ok(Orc::<'a, T> {
                    pointer_data: pointer_data,
                    weight_exp: Cell::new(MAX_WEIGHT_EXP),
                    lifetime_and_type: PhantomData,
                });
            }
        }
        Err("Out of memory")
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
fn construct_pointer_to_mut<T>(pointer: [u8; PTR_SIZE - 1], weight: u8) -> *mut OrcInner<T> {
    unsafe {
        let p: usize = transmute((pointer, weight));
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
mod test {
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
        let counter = Cell::new(test_size);

        let heap = OrcHeap::with_capacity(test_size);

        for _ in 0..test_size {
            let o = heap.alloc(DropTest(&counter)).unwrap();
        }
        assert_eq!(counter.get(), 0);
    }

    #[test]
    #[allow(unused_variables)]
    fn test_heap_freed() {
        let test_size = 2;
        let counter = Cell::new(5);

        let heap = OrcHeap::with_capacity(test_size);

        {
            let a = heap.alloc(DropTest(&counter)).unwrap();
            let b = heap.alloc(DropTest(&counter)).unwrap();
        }
        // now the heap should be freed and the allocations should be possible
        let c = heap.alloc(DropTest(&counter)).unwrap();
        let d = heap.alloc(DropTest(&counter)).unwrap();
        assert_eq!(counter.get(), 3); // a and b are dropped

        // and this must fail
        assert!(heap.alloc(DropTest(&counter)).is_err())
    }


}
