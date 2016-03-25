extern crate orc;
extern crate crossbeam;

use orc::OrcPool;



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


    let a = OrcPool::<Vec<TestType>>::new();
    
    let b = a.alloc(vec![TestType::new(1), TestType::new(2), TestType::new(3)]).unwrap();
	println!("{:?}", *b);	

    let c = b.clone();


	// println!("{:?}", b);	
	// println!("{:?}", c);	

	    
	// crossbeam::scope(|scope| {
	//     let second_thread_handle = scope.spawn(|| {
	//     	let d = a.alloc(vec![TestType::new(7), TestType::new(8), TestType::new(9)]);
	//     	println!("{:?}", c.clone()); 	
	//     	println!("From second thread");
	// 	    println!("{:?}", d);	
	// 	    d
	//     });

	//    	let d = second_thread_handle.join();
	// 	println!("value d created in 2. thread {:?}", d);	

	// });
}
