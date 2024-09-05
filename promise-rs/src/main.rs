use std::{io::Read, sync::{atomic::{AtomicI32, Ordering}, Arc}};

use promise_rs::Promise;

fn main() {
    let count = Arc::new(AtomicI32::new(0));
    let look_count = count.clone();
    for _ in 0..10 {
        let count = count.clone();
        let _: Promise<_, ()> = Promise::new(|rs| {
            rs.resolve(1);
        })
        .then(move |v| {
            count.fetch_add(1, Ordering::Relaxed);
            println!("v == {v}");
            Promise::new(move |rs| {
				//std::thread::sleep(std::time::Duration::from_secs(1));
                rs.resolve(v + 1)
                //rs.reject(PhantomData::<i32>::default());
            })
        })
        .then(|v| {
            let _: Promise<(), ()> = v.then(|v2| {
                println!("v2 == {v2}");
            });
        });
    }
    let mut buf = [0u8; 1];
    std::io::stdin().read(&mut buf[..]).unwrap();
    println!("total  = {}", look_count.load(Ordering::Relaxed));
}