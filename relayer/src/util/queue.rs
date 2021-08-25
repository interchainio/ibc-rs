use alloc::collections::VecDeque;
use core::cell::RefCell;

// A lightweight wrapper type to RefCell<VecDeque<T>> so that
// we can safely mutate it with regular reference instead of
// mutable reference. We only expose subset of VecDeque methods
// that does not return any inner reference, so that the RefCell
// can never panic caused by simultaneous borrow and borrow_mut.
pub struct Queue<T>(RefCell<VecDeque<T>>);

impl<T> Queue<T> {
    pub fn new() -> Self {
        Queue(RefCell::new(VecDeque::new()))
    }

    pub fn pop_front(&self) -> Option<T> {
        self.0.borrow_mut().pop_front()
    }

    pub fn pop_back(&self) -> Option<T> {
        self.0.borrow_mut().pop_back()
    }

    pub fn push_back(&self, val: T) {
        self.0.borrow_mut().push_front(val)
    }

    pub fn push_front(&self, val: T) {
        self.0.borrow_mut().push_front(val)
    }

    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    pub fn into_vec(self) -> VecDeque<T> {
        self.0.into_inner()
    }

    pub fn replace(&self, queue: VecDeque<T>) {
        self.0.replace(queue);
    }

    pub fn take(&self) -> VecDeque<T> {
        self.0.take()
    }
}

impl<T: Clone> Queue<T> {
    pub fn clone_vec(&self) -> VecDeque<T> {
        self.0.borrow().clone()
    }
}

impl<T: Clone> Clone for Queue<T> {
    fn clone(&self) -> Queue<T> {
        Queue(RefCell::new(self.0.borrow().clone()))
    }
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self::new()
    }
}
