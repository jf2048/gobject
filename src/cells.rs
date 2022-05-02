use crate::{
    OnceCell, ParamSpecBuildable, ParamStore, ParamStoreBorrow, ParamStoreBorrowMut,
    ParamStoreRead, ParamStoreWrite, ParamStoreWriteChanged,
};
use glib::{
    clone::Downgrade,
    value::{
        FromValue, ValueType, ValueTypeChecker, ValueTypeMismatchOrNoneError, ValueTypeOptional,
    },
    ObjectType, ToValue, Value, WeakRef,
};
use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
};

/// A cell holding an `Option<T>`. This should only be used with boxed/object properties using the
/// `CONSTRUCT` flag. The [`crate::ParamStoreRead`] implementation will panic if
/// the cell is not written to at least once.
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

impl<T: ParamSpecBuildable> ParamSpecBuildable for ConstructCell<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ValueType> ParamStore for ConstructCell<T> {
    type Type = T;
}
impl<T> ParamStoreRead for ConstructCell<T>
where
    T: ValueType + Clone,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        self.borrow().clone()
    }
    fn get_value(&self) -> Value {
        self.borrow().to_value()
    }
}
impl<'a, T> ParamStoreBorrow<'a> for ConstructCell<T>
where
    T: 'a,
{
    type BorrowType = std::cell::Ref<'a, T>;

    fn borrow(&'a self) -> Self::BorrowType {
        std::cell::Ref::map((**self).borrow(), |r| {
            r.as_ref().expect("ConstructCell borrowed before write")
        })
    }
}
impl<'a, T> ParamStoreWrite<'a> for ConstructCell<T>
where
    T: ValueType,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.replace(Some(value));
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for ConstructCell<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        let mut storage = (**self).borrow_mut();
        let old = std::mem::replace(storage.deref_mut(), Some(value));
        old != *storage
    }
}
impl<'a, T> ParamStoreBorrowMut<'a> for ConstructCell<T>
where
    T: 'a,
{
    type BorrowMutType = std::cell::RefMut<'a, T>;

    fn borrow_mut(&'a self) -> Self::BorrowMutType {
        std::cell::RefMut::map((**self).borrow_mut(), |r| {
            r.as_mut().expect("ConstructCell borrowed before write")
        })
    }
}

/// A cell holding a `T`. This should only be used with boxed/object properties using the
/// `CONSTRUCT` flag and implementing `Default`. If a NULL value is passed to
/// [`crate::ParamStoreWrite::set_value`], the value will be reset to `Default::default`.
#[derive(Debug)]
#[repr(transparent)]
pub struct ConstructDefaultCell<T>(RefCell<T>);

impl<T: Default> ConstructDefaultCell<T> {
    pub fn new() -> Self {
        Self(RefCell::new(Default::default()))
    }
}
impl<T: Default> Default for ConstructDefaultCell<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> From<T> for ConstructDefaultCell<T> {
    fn from(t: T) -> Self {
        Self(RefCell::new(t))
    }
}
impl<T> Deref for ConstructDefaultCell<T> {
    type Target = RefCell<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ParamSpecBuildable> ParamSpecBuildable for ConstructDefaultCell<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ValueType> ParamStore for ConstructDefaultCell<T> {
    type Type = T;
}
impl<T> ParamStoreRead for ConstructDefaultCell<T>
where
    T: ValueType + Clone,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        self.borrow().clone()
    }
    fn get_value(&self) -> Value {
        self.borrow().to_value()
    }
}
impl<'a, T> ParamStoreBorrow<'a> for ConstructDefaultCell<T>
where
    T: 'a,
{
    type BorrowType = std::cell::Ref<'a, T>;

    fn borrow(&'a self) -> Self::BorrowType {
        (**self).borrow()
    }
}
impl<'a, T, C, E> ParamStoreWrite<'a> for ConstructDefaultCell<T>
where
    T: for<'b> FromValue<'b, Checker = C>
        + ValueTypeOptional
        + Default
        + glib::StaticType
        + 'static,
    C: ValueTypeChecker<Error = ValueTypeMismatchOrNoneError<E>>,
    E: std::error::Error + Send + Sized + 'static,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.replace(value);
    }
    fn set_value(&'a self, value: &'a Value) {
        self.replace(value.get::<Option<T>>().unwrap().unwrap_or_default());
    }
}
impl<'a, T, C, E> ParamStoreWriteChanged<'a> for ConstructDefaultCell<T>
where
    T: for<'b> FromValue<'b, Checker = C>
        + ValueTypeOptional
        + Default
        + glib::StaticType
        + PartialEq
        + 'static,
    C: ValueTypeChecker<Error = ValueTypeMismatchOrNoneError<E>>,
    E: std::error::Error + Send + Sized + 'static,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        let mut storage = self.borrow_mut();
        let old = std::mem::replace(storage.deref_mut(), value);
        old != *storage
    }
}
impl<'a, T> ParamStoreBorrowMut<'a> for ConstructDefaultCell<T>
where
    T: 'a,
{
    type BorrowMutType = std::cell::RefMut<'a, T>;

    fn borrow_mut(&'a self) -> Self::BorrowMutType {
        (**self).borrow_mut()
    }
}

/// A cell holding a `T` that can be set only once. This should only be used with boxed/object
/// properties using the `CONSTRUCT_ONLY` flag. If the cell is not written to at least once, the
/// [`crate::ParamStoreRead::get_owned`] implementation will panic , but the
/// [`crate::ParamStoreRead::get_value`] implementation will return a NULL `glib::Value` for
/// compatibility with the C API.
#[derive(Debug)]
#[repr(transparent)]
pub struct ConstructOnlyCell<T>(OnceCell<T>);

impl<T> ConstructOnlyCell<T> {
    pub fn new() -> Self {
        Self(OnceCell::new())
    }
}
impl<T> Default for ConstructOnlyCell<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> From<T> for ConstructOnlyCell<T> {
    fn from(t: T) -> Self {
        Self(t.into())
    }
}
impl<T> Deref for ConstructOnlyCell<T> {
    type Target = OnceCell<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ParamSpecBuildable> ParamSpecBuildable for ConstructOnlyCell<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ValueType> ParamStore for ConstructOnlyCell<T> {
    type Type = T;
}
impl<T> ParamStoreRead for ConstructOnlyCell<T>
where
    T: ValueTypeOptional + Clone,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        self.borrow().clone()
    }
    fn get_value(&self) -> Value {
        self.get().to_value()
    }
}
impl<'a, T> ParamStoreBorrow<'a> for ConstructOnlyCell<T>
where
    T: 'a,
{
    type BorrowType = &'a T;

    fn borrow(&'a self) -> Self::BorrowType {
        self.get()
            .unwrap_or_else(|| panic!("`get()` called on uninitialized ConstructOnlyCell"))
    }
}
impl<'a, T> ParamStoreWrite<'a> for ConstructOnlyCell<T>
where
    T: ValueType,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.set(value)
            .unwrap_or_else(|_| panic!("set() called on initialized ConstructOnlyCell"));
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for ConstructOnlyCell<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        self.set_owned(value);
        true
    }
}

/// A cell holding a `T` that can be set only once. This should only be used with properties using
/// the `CONSTRUCT_ONLY` flag and implementing `Default`. If the cell is not written to, the
/// [`crate::ParamStoreRead::get_value`] implementation returns `Default::default()`.
#[derive(Debug)]
#[repr(transparent)]
pub struct ConstructOnlyDefaultCell<T>(OnceCell<T>);

impl<T> ConstructOnlyDefaultCell<T> {
    pub fn new() -> Self {
        Self(OnceCell::new())
    }
}
impl<T> Default for ConstructOnlyDefaultCell<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> From<T> for ConstructOnlyDefaultCell<T> {
    fn from(t: T) -> Self {
        Self(t.into())
    }
}
impl<T> Deref for ConstructOnlyDefaultCell<T> {
    type Target = OnceCell<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ParamSpecBuildable> ParamSpecBuildable for ConstructOnlyDefaultCell<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ValueType> ParamStore for ConstructOnlyDefaultCell<T> {
    type Type = T;
}
impl<T> ParamStoreRead for ConstructOnlyDefaultCell<T>
where
    T: ValueType + Clone + Default,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        self.borrow().clone()
    }
    fn get_value(&self) -> Value {
        if let Some(v) = self.get() {
            v.to_value()
        } else {
            T::default().to_value()
        }
    }
}
impl<'a, T> ParamStoreBorrow<'a> for ConstructOnlyDefaultCell<T>
where
    T: 'a,
{
    type BorrowType = &'a T;

    fn borrow(&'a self) -> Self::BorrowType {
        self.get()
            .unwrap_or_else(|| panic!("`get()` called on uninitialized ConstructOnlyDefaultCell"))
    }
}
impl<'a, T> ParamStoreWrite<'a> for ConstructOnlyDefaultCell<T>
where
    T: ValueType,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.set(value)
            .unwrap_or_else(|_| panic!("set() called on initialized ConstructOnlyDefaultCell"));
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for ConstructOnlyDefaultCell<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        self.set_owned(value);
        true
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

impl<T: ParamSpecBuildable + glib::ObjectType> ParamSpecBuildable for WeakCell<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ObjectType> ParamStore for WeakCell<T> {
    type Type = T;
}
impl<T> ParamStoreRead for WeakCell<T>
where
    T: ObjectType,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        self.upgrade().expect("Failed to upgrade WeakRef")
    }
}
impl<'a, T> ParamStoreWrite<'a> for WeakCell<T>
where
    T: ObjectType,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.set(Some(&value));
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for WeakCell<T>
where
    T: ObjectType + PartialEq,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        let old = self.get_owned();
        self.set(Some(&value));
        old != value
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

impl<T: ParamSpecBuildable + glib::clone::Downgrade> ParamSpecBuildable for DowngradeCell<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T> ParamStore for DowngradeCell<T>
where
    T: ValueType + glib::clone::Downgrade,
    <T as glib::clone::Downgrade>::Weak: glib::clone::Upgrade<Strong = T>,
{
    type Type = T;
}
impl<T> ParamStoreRead for DowngradeCell<T>
where
    T: ValueType + glib::clone::Downgrade,
    <T as glib::clone::Downgrade>::Weak: glib::clone::Upgrade<Strong = T>,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        glib::clone::Upgrade::upgrade(&*self.borrow()).expect("Failed to upgrade weak reference")
    }
}
impl<'a, T> ParamStoreWrite<'a> for DowngradeCell<T>
where
    T: ValueType + glib::clone::Downgrade,
    <T as glib::clone::Downgrade>::Weak: glib::clone::Upgrade<Strong = T>,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.replace(glib::clone::Downgrade::downgrade(&value));
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for DowngradeCell<T>
where
    T: ValueType + glib::clone::Downgrade + PartialEq,
    <T as glib::clone::Downgrade>::Weak: glib::clone::Upgrade<Strong = T>,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        let old = self.get_owned();
        self.replace(glib::clone::Downgrade::downgrade(&value));
        old != value
    }
}
