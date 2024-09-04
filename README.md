# promise-rs

````rust
use std::{
    cell::UnsafeCell,
    io::Read,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};
const PENDING: u8 = 0;
const FILL: u8 = 1;
const READY: u8 = 2;
const FINISH: u8 = 3;
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

struct Rs<U, T>(
    Arc<AtomicU8>,
    Arc<Cell<U>>,
    Arc<Cell<Box<dyn Fn(U) -> T + Send + 'static>>>,
);

impl<U, T> Rs<U, T> {
    fn resolve(&self, v: U) {
        self.1.set(v);
        if self
            .0
            .compare_exchange(PENDING, READY, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return;
        }
        match self
            .0
            .compare_exchange(FILL, FINISH, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_v) => {
                //println!("invoke by resolve");
                self.invoke();
            }
            Err(_v) => {
                unreachable!();
            }
        }
    }
    fn invoke(&self) {
        let v = self.1.take().unwrap();
        (*(*self.2).as_ref().unwrap())(v);
    }
}
struct Promise<U, Ret> {
    status: Arc<AtomicU8>,
    value: Arc<Cell<U>>,
    call_back: Arc<Cell<Box<dyn Fn(U) -> Ret + Send + 'static>>>,
}
impl<U: Send + 'static, Ret: Send + 'static> Promise<U, Ret> {
    fn new<F: Fn(Rs<U, Ret>) + 'static + Send>(f: F) -> Self {
        let status = Arc::new(AtomicU8::new(0));
        let status_cp = status.clone();
        let value = Arc::new(Cell::new());
        let value_cp = value.clone();
        let call_back = Arc::new(Cell::new());
        let call_back_cp = call_back.clone();
        let rs = Rs(status_cp, value_cp, call_back_cp);
        std::thread::spawn(move || {
            f(rs);
        });
        Promise {
            status,
            value,
            call_back,
        }
    }
    fn invoke(&self) {
        let v = self.value.take().unwrap();
        (*(*self.call_back).as_ref().unwrap())(v);
    }
    fn then<F: Fn(U) -> Ret + Send + Sync + 'static>(&self, f: F) {
        self.call_back.set(Box::new(f));
        if self
            .status
            .compare_exchange(READY, FILL, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            //println!("do not need wait");
            self.invoke();
            return;
        }
        //std::thread::sleep(std::time::Duration::from_millis(5));
        match self
            .status
            .compare_exchange(PENDING, FILL, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_v) => {}
            Err(v) => {
                //println!("invoke compare error");
                if v == READY {
                    //println!("invoke by then");
                    self.invoke();
                    return;
                }
                unreachable!();
            }
        }
    }
}

fn main() {
    for _ in 0..1950 {
        Promise::new(|rs| {
            //std::thread::sleep(std::time::Duration::from_secs(1));
            //std::thread::sleep(std::time::Duration::from_millis(5));
            rs.resolve(1);
        })
        .then(|v| {
            println!("v == {v}");
        });
    }
    let mut buf = [0u8; 1];
    std::io::stdin().read(&mut buf[..]).unwrap();
}

````
