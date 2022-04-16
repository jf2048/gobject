/// A cell holding an `Option<T>`. The [`crate::ParamStoreRead`] implementation returns `T` and
/// will panic if the cell is not written to at least once.
#[derive(Debug)]
#[repr(transparent)]
pub struct ConstructCell<T>(std::cell::RefCell<Option<T>>);

impl<T> ConstructCell<T> {
    pub fn new() -> Self {
        Self(std::cell::RefCell::new(None))
    }
}

impl<T> Default for ConstructCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> From<T> for ConstructCell<T> {
    fn from(t: T) -> Self {
        Self(std::cell::RefCell::new(Some(t)))
    }
}

impl<T> ::std::ops::Deref for ConstructCell<T> {
    type Target = std::cell::RefCell<Option<T>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
