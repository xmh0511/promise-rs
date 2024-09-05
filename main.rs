use std::{
    cell::UnsafeCell, io::Read, marker::PhantomData, sync::{
        atomic::{AtomicI32, AtomicU8, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc,
    }
};
const PENDING: u8 = 0;
const FILL: u8 = 1;
const READY: u8 = 2;
const REJECT:u8 = 3;
const FINISH: u8 = 4;
struct Cell<T> {
    val: UnsafeCell<Option<T>>,
}

impl<T> Cell<T> {
    fn new() -> Self {
        Self {
            val: UnsafeCell::new(None),
        }
    }
    fn set(&self, v: T) {
        unsafe {
            let ptr = self.val.get();
            *ptr = Some(v);
        }
    }
    fn take(&self) -> Option<T> {
        unsafe { (*self.val.get()).take() }
    }
    fn as_ref(&self) -> Option<&T> {
        unsafe { (*self.val.get()).as_ref() }
    }
}

unsafe impl<T> Send for Cell<T> {}
unsafe impl<T> Sync for Cell<T> {}

struct Rs<Arg, Ret>(
    Arc<AtomicU8>,
    Arc<Cell<Arg>>,
    Arc<Cell<Box<dyn Fn(Arg) -> Ret + Send + 'static>>>,
    Sender<Ret>,
);

impl<Arg, Ret> Rs<Arg, Ret> {
    fn resolve(&self, v: Arg) {
        self.1.set(v);
        if self
            .0
            .compare_exchange(PENDING, READY, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            return;
        }
        match self
            .0
            .compare_exchange(FILL, FINISH, Ordering::AcqRel, Ordering::Relaxed)
        {
            Ok(_v) => {
                //println!("invoke by resolve");
                let r = self.invoke();
                self.3.send(r).unwrap();
            }
            Err(_v) => {
                unreachable!();
            }
        }
    }
	fn reject(&self,_:PhantomData<Arg>){
		self.0.store(REJECT, Ordering::Relaxed);
	}
    fn invoke(&self) -> Ret {
        let v = self.1.take().unwrap();
        (*(*self.2).as_ref().unwrap())(v)
    }
}
struct Promise<Arg, Ret> {
    status: Arc<AtomicU8>,
    value: Arc<Cell<Arg>>,
    call_back: Arc<Cell<Box<dyn Fn(Arg) -> Ret + Send + 'static>>>,
    recv: Receiver<Ret>,
}
impl<Arg: Send + 'static, Ret: Send + 'static> Promise<Arg, Ret> {
    fn new<F: FnOnce(Rs<Arg, Ret>) + 'static + Send>(f: F) -> Self {
        let status = Arc::new(AtomicU8::new(0));
        let status_cp = status.clone();
        let value = Arc::new(Cell::new());
        let value_cp = value.clone();
        let call_back = Arc::new(Cell::new());
        let call_back_cp = call_back.clone();
        let (tx, recv) = channel::<Ret>();
        let rs = Rs(status_cp, value_cp, call_back_cp, tx);
        std::thread::spawn(move || {
            f(rs);
        });
        Promise {
            status,
            value,
            call_back,
            recv,
        }
    }
    fn invoke(&self) -> Ret {
        let v = self.value.take().unwrap();
        (*(*self.call_back).as_ref().unwrap())(v)
    }
    fn then<D : Send + 'static, F: Fn(Arg) -> Ret + Send + Sync + 'static>(
        self,
        f: F,
    ) -> Promise<Ret, D> {
        //std::thread::sleep(std::time::Duration::from_millis(5));
        self.call_back.set(Box::new(f));
        if self
            .status
            .compare_exchange(PENDING, FILL, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            let rx = self.recv;
            return Promise::new(move |rs| {
                match rx.recv() {
                    Ok(v) => rs.resolve(v),
                    Err(_) => {
						//reject
					},
                };
            });
        }
        match self
            .status
            .compare_exchange(READY, FINISH, Ordering::AcqRel, Ordering::Relaxed)
        {
            Ok(_v) => {
                //println!("invoke by then");
                let r = self.invoke();
                Promise::new(move |rs| rs.resolve(r))
            }
            Err(v) => {
				if v == REJECT{
					return Promise::new(move |_| {});
				}
                unreachable!();
            }
        }
    }
}

fn main() {
    let count = Arc::new(AtomicI32::new(0));
    let look_count = count.clone();
    for _ in 0..10 {
        let count = count.clone();
        let _c: Promise<_, ()> = Promise::new(|rs| {
            //std::thread::sleep(std::time::Duration::from_secs(1));
            //std::thread::sleep(std::time::Duration::from_millis(50));
            rs.resolve(1);
        })
        .then(move |v| {
            count.fetch_add(1, Ordering::Relaxed);
            println!("v == {v}");
			//std::thread::sleep(std::time::Duration::from_secs(1));
			Promise::new(move |rs|{
				rs.resolve(v+1)
				//rs.reject(PhantomData::<i32>::default());
			})
        })
        .then(|v| {
			let _:Promise<(), ()> = v.then(|v2|{
				println!("v2 == {v2}");
			});
        });
    }
    let mut buf = [0u8; 1];
    std::io::stdin().read(&mut buf[..]).unwrap();
    println!("total  = {}", look_count.load(Ordering::Relaxed));
}
