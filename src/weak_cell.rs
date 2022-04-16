/// A cell holding a `WeakRef`. The [`crate::ParamStoreRead`] implementation will panic if
/// upgrading the weak ref fails. Only use this if the property is read-only, and if it is
/// guaranteed that something else stored in the object is holding a strong reference to `T`.
#[derive(Debug)]
#[repr(transparent)]
pub struct WeakCell<T: glib::ObjectType>(glib::WeakRef<T>);

impl<T: glib::ObjectType> WeakCell<T> {
    pub fn new() -> Self {
        Self(glib::WeakRef::new())
    }
}

impl<T: glib::ObjectType> Default for WeakCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: glib::ObjectType> From<T> for WeakCell<T> {
    fn from(obj: T) -> Self {
        let weak = Self::new();
        weak.set(Some(&obj));
        weak
    }
}

impl<T: glib::ObjectType> std::ops::Deref for WeakCell<T> {
    type Target = glib::WeakRef<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
