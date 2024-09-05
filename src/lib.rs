use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    sync::{
        atomic::{AtomicU8, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
};
pub const PENDING: u8 = 0;
pub const FILL: u8 = 1;
pub const READY: u8 = 2;
pub const REJECT: u8 = 3;
pub const FINISH: u8 = 4;
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

pub struct Rs<Arg, Ret>(
    Arc<AtomicU8>,
    Arc<Cell<Arg>>,
    Arc<Cell<Box<dyn Fn(Arg) -> Ret + Send + 'static>>>,
    Sender<Ret>,
);

impl<Arg, Ret> Rs<Arg, Ret> {
    pub fn resolve(&self, v: Arg) {
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
    pub fn reject(&self, _: PhantomData<Arg>) {
        self.0.store(REJECT, Ordering::Relaxed);
    }
    fn invoke(&self) -> Ret {
        let v = self.1.take().unwrap();
        (*(*self.2).as_ref().unwrap())(v)
    }
}
pub struct Promise<Arg, Ret> {
    status: Arc<AtomicU8>,
    value: Arc<Cell<Arg>>,
    call_back: Arc<Cell<Box<dyn Fn(Arg) -> Ret + Send + 'static>>>,
    recv: Receiver<Ret>,
}
impl<Arg: Send + 'static, Ret: Send + 'static> Promise<Arg, Ret> {
    pub fn new<F: FnOnce(Rs<Arg, Ret>) + 'static + Send>(f: F) -> Self {
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
    pub fn then<D: Send + 'static, F: Fn(Arg) -> Ret + Send + Sync + 'static>(
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
                    }
                };
            });
        }
        match self
            .status
            .compare_exchange(READY, FINISH, Ordering::AcqRel, Ordering::Relaxed)
        {
            Ok(_v) => {
                //println!("invoke by then");
                Promise::new(move |rs| {
                    let r = self.invoke();
                    rs.resolve(r)
                })
            }
            Err(v) => {
                if v == REJECT {
                    return Promise::new(move |_| {});
                }
                if v == FINISH {
                    panic!(
                        "the promise has been exhausted, complete promise cannot be executed again"
                    );
                }
                unreachable!(
                    "Change READY to FINISH in `then` cannot have other cases except REJECT"
                );
            }
        }
    }
}
