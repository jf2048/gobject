#[derive(Debug)]
#[repr(transparent)]
pub struct ConstructCell<T>(std::cell::RefCell<Option<T>>);

impl<T> ConstructCell<T> {
    pub fn new(value: T) -> Self {
        Self(std::cell::RefCell::new(Some(value)))
    }
    pub fn new_empty() -> Self {
        Self(std::cell::RefCell::new(None))
    }
    pub fn borrow(&self) -> std::cell::Ref<'_, T> {
        std::cell::Ref::map(self.0.borrow(), |r| r.as_ref().unwrap())
    }
    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, T> {
        std::cell::RefMut::map(self.0.borrow_mut(), |r| r.as_mut().unwrap())
    }
    pub fn replace(&self, t: T) -> Option<T> {
        self.0.replace(Some(t))
    }
}

impl<T> Default for ConstructCell<T> {
    fn default() -> Self {
        Self::new_empty()
    }
}
