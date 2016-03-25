extern crate orc;
extern crate crossbeam;

use orc::Arena;



#[derive(Debug)]
struct TestType(usize);

impl TestType {
	fn new(v: usize) -> TestType {
		TestType(v)
	}
}

impl Drop for TestType {
    fn drop(&mut self) {
        println!("Drop TestType with Value: {}", self.0);
    }
}


fn main() {


    let a = Arena::<Vec<TestType>>::new();
    
    let b = a.alloc(vec![TestType::new(1), TestType::new(2), TestType::new(3)]);
    let c = b.clone();
    let d = b.clone();


	println!("{:?}", b);	
	println!("{:?}", c);	
	println!("{:?}", d);	

	    
	// crossbeam::scope(|scope| {
	//     let second_thread_handle = scope.spawn(|| {
	//     	let d = a.alloc(vec![TestType::new(7), TestType::new(8), TestType::new(9)]);
	//     	println!("From second thread");
	// 	    // println!("{:?}", c);	
	// 	    d
	//     });

	//    	let d = second_thread_handle.join();
	// 	// println!("value d created in 2. thread {:?}", d);	

	// });
}
