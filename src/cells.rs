use glib::{clone::Downgrade, ObjectType, WeakRef};
use std::{cell::RefCell, ops::Deref};

/// A cell holding an `Option<T>`. The [`crate::ParamStoreRead`] implementation returns `T` and
/// will panic if the cell is not written to at least once.
#[derive(Debug)]
#[repr(transparent)]
pub struct ConstructCell<T>(RefCell<Option<T>>);

impl<T> ConstructCell<T> {
    pub fn new() -> Self {
        Self(RefCell::new(None))
    }
}

impl<T> Default for ConstructCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> From<T> for ConstructCell<T> {
    fn from(t: T) -> Self {
        Self(RefCell::new(Some(t)))
    }
}

impl<T> Deref for ConstructCell<T> {
    type Target = RefCell<Option<T>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A cell holding a `WeakRef`. The [`crate::ParamStoreRead`] implementation will panic if
/// upgrading the weak ref fails. Only use this if the property is read-only, and if it is
/// guaranteed that something else stored in the object is holding a strong reference to `T`.
#[derive(Debug)]
#[repr(transparent)]
pub struct WeakCell<T: ObjectType>(WeakRef<T>);

impl<T: ObjectType> WeakCell<T> {
    pub fn new() -> Self {
        Self(WeakRef::new())
    }
}

impl<T: ObjectType> Default for WeakCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ObjectType> From<T> for WeakCell<T> {
    fn from(obj: T) -> Self {
        let weak = Self::new();
        weak.set(Some(&obj));
        weak
    }
}

impl<T: ObjectType> Deref for WeakCell<T> {
    type Target = WeakRef<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A cell holding a weak reference obtained through `glib::clone::Downgrade`. The
/// [`crate::ParamStoreRead`] implementation will panic if upgrading the weak ref fails. Only use
/// this if the property is read-only, and if it is guaranteed that something else stored in the
/// object is holding a strong reference to `T`.
#[derive(Debug)]
#[repr(transparent)]
pub struct DowngradeCell<T: Downgrade>(RefCell<T::Weak>);

impl<T: Downgrade> DowngradeCell<T>
where
    T::Weak: Default,
{
    pub fn new() -> Self {
        Self(RefCell::new(Default::default()))
    }
}

impl<T: Downgrade> Default for DowngradeCell<T>
where
    T::Weak: Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Downgrade> From<T> for DowngradeCell<T> {
    fn from(obj: T) -> Self {
        Self(RefCell::new(obj.downgrade()))
    }
}

impl<T: Downgrade> Deref for DowngradeCell<T> {
    type Target = RefCell<T::Weak>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
