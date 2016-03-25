use std::mem::transmute;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::marker::PhantomData;
use std::cell::Cell;

// change to const PTR_SIZE: usize = size_of::<usize>() as soon it's a const fn
#[cfg(target_pointer_width = "32")]
const PTR_SIZE: usize = 4;
#[cfg(target_pointer_width = "64")]
const PTR_SIZE: usize = 8;

const MAX_WEIGHT_EXP: u8 = PTR_SIZE as u8 * 8 - 1;


pub struct Arena<T> {
    heap: Vec<OrcInner<T>>
}

// Debug
impl<'a, T> Drop for Arena<T> {
    fn drop(&mut self) {
        println!("Drop Arena");
    }
}

#[derive(Debug)]
enum OrcInner<T> {
    Some{
        weight: AtomicUsize,
        data: T,
    },
    None,
}



#[derive(Debug)]
pub struct Orc<'a, T: 'a> {
	pointer_data: [u8; PTR_SIZE-1], // the ptr is in little endian byteorder
	weight_exp: Cell<u8>,
	lifetime_and_type: PhantomData<&'a T>
}


impl<'a, T> Drop for Orc<'a, T> { 
	fn drop(&mut self) {
		// Debug
		println!("Drop Orc");

    	if self.weight_exp.get() == MAX_WEIGHT_EXP {
    		let slot = construct_pointer_to_mut::<T>(self.pointer_data, 0);
			unsafe {
			    *slot = OrcInner::None  
			}
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
			}
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

impl<'a, T: Debug> Arena<T> { // TODO: Remove debug trait
    pub fn new() -> Arena<T> {
		const DEFAULT_HEAP_SIZE: usize = 10;

		let mut heap = Vec::with_capacity(DEFAULT_HEAP_SIZE);
		// it is important that no other push operations on any of theses vectors are performed
		for _ in 0 .. DEFAULT_HEAP_SIZE {
			heap.push(OrcInner::None);
		}
		// make sure that all pointers have enough headroom to store the weight
		let (_, weight) = deconstruct_pointer(heap.iter().nth(DEFAULT_HEAP_SIZE-1).unwrap());
		assert_eq!(weight, 0);

		println!("new arena {:?}", deconstruct_pointer(heap.iter().nth(DEFAULT_HEAP_SIZE-1).unwrap()));	

        Arena::<T> {
            heap: heap,
        }
    }
    
    pub fn alloc(&'a self, value: T) -> Orc<T> {

    	// find an empty slot
    	if let Some(position) = (&self.heap).iter().position(|x| match x { &OrcInner::None => true, _ => false} ) {
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
		        return Orc::<'a, T> {
		            pointer_data: pointer_data,
		            weight_exp: Cell::new(MAX_WEIGHT_EXP),
		            lifetime_and_type: PhantomData,
		        }
		    }
    	}
		panic!("gc collection");
    }
}


// helper functions

#[inline(always)]
fn deconstruct_pointer<T>(p: &OrcInner<T>) -> ([u8; PTR_SIZE-1], u8) {
	unsafe {
    	let p: usize = transmute(p);
 		transmute(usize::from_le(p)) // NOOP on little endian machines
    }	
}

#[inline(always)]
fn construct_pointer_to_mut<T>(pointer: [u8; PTR_SIZE-1], weight: u8 ) -> *mut OrcInner<T> {
	unsafe {
    	let p: usize = transmute((pointer, weight));
 		transmute(usize::from_le(p)) // NOOP on little endian machines	
	}
}

#[inline(always)]
fn construct_pointer<'a, T>(pointer: [u8; PTR_SIZE-1], weight: u8 ) -> &'a OrcInner<T> {
	unsafe {
    	let p: usize = transmute((pointer, weight));
 		transmute(usize::from_le(p)) // NOOP on little endian machines	
	}
}

#[inline(always)]
fn two_two_the(exp: u8) -> usize {
	1usize << exp 
}

#[test]
fn test_two_two_the() {
	assert_eq!(two_two_the(0), 1);
	assert_eq!(two_two_the(1), 2);
	assert_eq!(two_two_the(8), 256);
}


// #[test]
// fn it_works() {
// }
