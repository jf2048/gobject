use crate::{OnceBool, OnceBox, OnceCell, SyncOnceCell};
use glib::{
    value::{
        FromValue, ValueType, ValueTypeChecker, ValueTypeMismatchOrNoneError, ValueTypeOptional,
    },
    ObjectType, ToValue, Value,
};
use std::{ops::DerefMut, sync::atomic::Ordering};

pub trait ParamStore {
    type Type: ValueType;
}
pub trait ParamStoreRead: ParamStore {
    type ReadType: ToValue;
    fn get_owned(&self) -> Self::ReadType;
    fn get_value(&self) -> glib::Value {
        self.get_owned().to_value()
    }
}
pub trait ParamStoreBorrow<'a>: ParamStore {
    type BorrowType;

    fn borrow(&'a self) -> Self::BorrowType;
}
pub trait ParamStoreWrite<'a>: ParamStore {
    type WriteType: FromValue<'a>;
    fn set_owned(&'a self, value: Self::WriteType);
    fn set_value(&'a self, value: &'a Value) {
        self.set_owned(value.get().expect("invalid value for property"));
    }
}
pub trait ParamStoreWriteChanged<'a>: ParamStoreWrite<'a> {
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool;
}

impl<T: ValueType> ParamStore for std::cell::Cell<T> {
    type Type = T;
}
impl<T> ParamStoreRead for std::cell::Cell<T>
where
    T: ValueType + Copy,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        std::cell::Cell::get(self)
    }
}
impl<'a, T> ParamStoreWrite<'a> for std::cell::Cell<T>
where
    T: ValueType,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.replace(value);
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::cell::Cell<T>
where
    T: ValueType + PartialEq + Copy,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        let old = self.replace(value);
        old != self.get()
    }
}

impl<T: ValueType> ParamStore for std::cell::RefCell<T> {
    type Type = T;
}
impl<T> ParamStoreRead for std::cell::RefCell<T>
where
    T: ValueType + Clone,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        self.borrow().clone()
    }
    fn get_value(&self) -> glib::Value {
        self.borrow().to_value()
    }
}
impl<'a, T> ParamStoreBorrow<'a> for std::cell::RefCell<T>
where
    T: ValueType + 'a,
{
    type BorrowType = std::cell::Ref<'a, T>;

    fn borrow(&'a self) -> Self::BorrowType {
        std::cell::RefCell::borrow(self)
    }
}
impl<'a, T> ParamStoreWrite<'a> for std::cell::RefCell<T>
where
    T: ValueType,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.replace(value);
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::cell::RefCell<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        let mut storage = self.borrow_mut();
        let old = std::mem::replace(storage.deref_mut(), value);
        old != *storage
    }
}

impl<T: ValueType> ParamStore for std::sync::Mutex<T> {
    type Type = T;
}
impl<T> ParamStoreRead for std::sync::Mutex<T>
where
    T: ValueType + Clone,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        self.borrow().clone()
    }
    fn get_value(&self) -> glib::Value {
        self.borrow().to_value()
    }
}
impl<'a, T> ParamStoreBorrow<'a> for std::sync::Mutex<T>
where
    T: ValueType + 'a,
{
    type BorrowType = std::sync::MutexGuard<'a, T>;

    fn borrow(&'a self) -> Self::BorrowType {
        self.lock().unwrap()
    }
}
impl<'a, T> ParamStoreWrite<'a> for std::sync::Mutex<T>
where
    T: ValueType,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        *self.lock().unwrap() = value;
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::sync::Mutex<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        let mut storage = self.lock().unwrap();
        let old = std::mem::replace(storage.deref_mut(), value);
        old != *storage
    }
}

impl<T: ValueType> ParamStore for std::sync::RwLock<T> {
    type Type = T;
}
impl<T> ParamStoreRead for std::sync::RwLock<T>
where
    T: ValueType + Clone,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        self.borrow().clone()
    }
    fn get_value(&self) -> glib::Value {
        self.borrow().to_value()
    }
}
impl<'a, T> ParamStoreBorrow<'a> for std::sync::RwLock<T>
where
    T: ValueType + 'a,
{
    type BorrowType = std::sync::RwLockReadGuard<'a, T>;

    fn borrow(&'a self) -> Self::BorrowType {
        self.read().unwrap()
    }
}
impl<'a, T> ParamStoreWrite<'a> for std::sync::RwLock<T>
where
    T: ValueType,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        *self.write().unwrap() = value;
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::sync::RwLock<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        let mut storage = self.write().unwrap();
        let old = std::mem::replace(storage.deref_mut(), value);
        old != *storage
    }
}

impl<T: ValueType> ParamStore for OnceCell<T> {
    type Type = T;
}
impl<T> ParamStoreRead for OnceCell<T>
where
    T: ValueType + Clone,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        self.borrow().clone()
    }
    fn get_value(&self) -> glib::Value {
        self.borrow().to_value()
    }
}
impl<'a, T> ParamStoreBorrow<'a> for OnceCell<T>
where
    T: ValueType + 'a,
{
    type BorrowType = &'a T;

    fn borrow(&'a self) -> Self::BorrowType {
        self.get()
            .unwrap_or_else(|| panic!("`get()` called on uninitialized OnceCell"))
    }
}
impl<'a, T> ParamStoreWrite<'a> for OnceCell<T>
where
    T: ValueType,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.set(value)
            .unwrap_or_else(|_| panic!("set() called on initialized OnceCell"));
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for OnceCell<T>
where
    T: ValueType + PartialEq + Copy,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        self.set_owned(value);
        true
    }
}

impl<T: ValueType> ParamStore for SyncOnceCell<T> {
    type Type = T;
}
impl<T> ParamStoreRead for SyncOnceCell<T>
where
    T: ValueType + Clone,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        self.borrow().clone()
    }
    fn get_value(&self) -> glib::Value {
        self.borrow().to_value()
    }
}
impl<'a, T> ParamStoreBorrow<'a> for SyncOnceCell<T>
where
    T: ValueType + 'a,
{
    type BorrowType = &'a T;

    fn borrow(&'a self) -> Self::BorrowType {
        self.get()
            .unwrap_or_else(|| panic!("`get()` called on uninitialized OnceCell"))
    }
}
impl<'a, T> ParamStoreWrite<'a> for SyncOnceCell<T>
where
    T: ValueType,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.set(value)
            .unwrap_or_else(|_| panic!("set() called on initialized OnceCell"));
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for SyncOnceCell<T>
where
    T: ValueType + PartialEq + Copy,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        self.set_owned(value);
        true
    }
}

impl<T: ValueType> ParamStore for OnceBox<T> {
    type Type = T;
}
impl<T> ParamStoreRead for OnceBox<T>
where
    T: ValueTypeOptional + Clone,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        self.borrow().clone()
    }
    fn get_value(&self) -> glib::Value {
        self.get().to_value()
    }
}
impl<'a, T> ParamStoreBorrow<'a> for OnceBox<T>
where
    T: ValueType + 'a,
{
    type BorrowType = &'a T;

    fn borrow(&'a self) -> Self::BorrowType {
        self.get()
            .unwrap_or_else(|| panic!("`get()` called on uninitialized OnceBox"))
    }
}
impl<'a, T> ParamStoreWrite<'a> for OnceBox<T>
where
    T: ValueType,
{
    type WriteType = T;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.set(Box::new(value))
            .unwrap_or_else(|_| panic!("set() called on initialized OnceBox"));
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for OnceBox<T>
where
    T: ValueType + PartialEq + Copy,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        self.set_owned(value);
        true
    }
}

impl ParamStore for OnceBool {
    type Type = bool;
}
impl ParamStoreRead for OnceBool {
    type ReadType = bool;
    fn get_owned(&self) -> bool {
        self.get()
            .unwrap_or_else(|| panic!("`get()` called on uninitialized OnceBool"))
    }
}
impl<'a> ParamStoreWrite<'a> for OnceBool {
    type WriteType = bool;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.set(value)
            .unwrap_or_else(|_| panic!("set() called on initialized OnceBool"));
    }
}
impl<'a> ParamStoreWriteChanged<'a> for OnceBool {
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        self.set_owned(value);
        true
    }
}

macro_rules! atomic_type {
    ($ty:ty, $inner:ty) => {
        impl ParamStore for $ty {
            type Type = $inner;
        }
        impl ParamStoreRead for $ty {
            type ReadType = $inner;
            fn get_owned(&self) -> $inner {
                self.load(Ordering::Acquire)
            }
        }
        impl<'a> ParamStoreWrite<'a> for $ty {
            type WriteType = $inner;
            fn set_owned(&'a self, value: Self::WriteType) {
                self.store(value, Ordering::Release);
            }
        }
        impl<'a> ParamStoreWriteChanged<'a> for $ty {
            fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
                let old = self.swap(value, Ordering::Release);
                old != value
            }
        }
    };
}

atomic_type!(std::sync::atomic::AtomicBool, bool);
atomic_type!(std::sync::atomic::AtomicI8, i8);
atomic_type!(std::sync::atomic::AtomicI32, i32);
atomic_type!(std::sync::atomic::AtomicI64, i64);
atomic_type!(std::sync::atomic::AtomicU8, u8);
atomic_type!(std::sync::atomic::AtomicU32, u32);
atomic_type!(std::sync::atomic::AtomicU64, u64);

impl<T> ParamStore for std::sync::atomic::AtomicPtr<T> {
    type Type = glib::Pointer;
}
impl<T> ParamStoreRead for std::sync::atomic::AtomicPtr<T> {
    type ReadType = glib::Pointer;
    fn get_owned(&self) -> Self::ReadType {
        self.load(Ordering::Acquire) as glib::Pointer
    }
}
impl<'a, T> ParamStoreWrite<'a> for std::sync::atomic::AtomicPtr<T> {
    type WriteType = glib::Pointer;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.store(value as *mut T, Ordering::Release);
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::sync::atomic::AtomicPtr<T> {
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        let value = value as *mut T;
        let old = self.swap(value, Ordering::Release);
        old != value
    }
}

impl<T, C, E> ParamStore for glib::WeakRef<T>
where
    T: ObjectType
        + for<'a> FromValue<'a, Checker = C>
        + ValueTypeOptional
        + glib::StaticType
        + 'static,
    C: ValueTypeChecker<Error = ValueTypeMismatchOrNoneError<E>>,
    E: std::error::Error + Send + Sized + 'static,
{
    type Type = Option<T>;
}
impl<T, C, E> ParamStoreRead for glib::WeakRef<T>
where
    T: ObjectType
        + for<'b> FromValue<'b, Checker = C>
        + ValueTypeOptional
        + glib::StaticType
        + 'static,
    C: ValueTypeChecker<Error = ValueTypeMismatchOrNoneError<E>>,
    E: std::error::Error + Send + Sized + 'static,
{
    type ReadType = Option<T>;
    fn get_owned(&self) -> Self::ReadType {
        self.upgrade()
    }
}
impl<'a, T, C, E> ParamStoreWrite<'a> for glib::WeakRef<T>
where
    T: ObjectType
        + for<'b> FromValue<'b, Checker = C>
        + ValueTypeOptional
        + glib::StaticType
        + 'static,
    C: ValueTypeChecker<Error = ValueTypeMismatchOrNoneError<E>>,
    E: std::error::Error + Send + Sized + 'static,
{
    type WriteType = Option<T>;
    fn set_owned(&'a self, value: Self::WriteType) {
        self.set(value.as_ref());
    }
}
impl<'a, T, C, E> ParamStoreWriteChanged<'a> for glib::WeakRef<T>
where
    T: ObjectType
        + for<'b> FromValue<'b, Checker = C>
        + ValueTypeOptional
        + glib::StaticType
        + PartialEq
        + 'static,
    C: ValueTypeChecker<Error = ValueTypeMismatchOrNoneError<E>>,
    E: std::error::Error + Send + Sized + 'static,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        let old = self.upgrade();
        self.set(value.as_ref());
        old != value
    }
}

impl<T: ValueType> ParamStore for std::marker::PhantomData<T> {
    type Type = T;
}
impl<T> ParamStoreRead for std::marker::PhantomData<T>
where
    T: ValueType,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        unimplemented!("get() called on abstract property");
    }
}
impl<'a, T> ParamStoreWrite<'a> for std::marker::PhantomData<T>
where
    T: ValueType,
{
    type WriteType = T;
    fn set_owned(&'a self, _value: Self::WriteType) {
        unimplemented!("set() called on abstract property");
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::marker::PhantomData<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: Self::WriteType) -> bool {
        self.set_owned(value);
        false
    }
}

#[cfg(feature = "use_gtk4")]
impl<T> ParamStore for gtk4::TemplateChild<T>
where
    T: glib::ObjectType + glib::translate::FromGlibPtrNone<*mut <T as glib::ObjectType>::GlibType>,
{
    type Type = T;
}
#[cfg(feature = "use_gtk4")]
impl<T> ParamStoreRead for gtk4::TemplateChild<T>
where
    T: glib::ObjectType
        + ValueTypeOptional
        + glib::translate::FromGlibPtrNone<*mut <T as glib::ObjectType>::GlibType>,
{
    type ReadType = T;
    fn get_owned(&self) -> Self::ReadType {
        gtk4::TemplateChild::get(self)
    }
    fn get_value(&self) -> glib::Value {
        gtk4::TemplateChild::try_get(self).to_value()
    }
}
#[cfg(feature = "use_gtk4")]
impl<'a, T> ParamStoreBorrow<'a> for gtk4::TemplateChild<T>
where
    T: glib::ObjectType
        + glib::translate::FromGlibPtrNone<*mut <T as glib::ObjectType>::GlibType>
        + 'a,
{
    type BorrowType = &'a T;

    fn borrow(&'a self) -> Self::BorrowType {
        &*self
    }
}
