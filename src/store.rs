use crate::{ConstructCell, OnceBool, OnceBox, OnceCell, SyncOnceCell};
use glib::{ToValue, value::ValueType, Value};
use std::{ops::DerefMut, sync::atomic::Ordering};

pub trait ParamStore {
    type Type: ValueType;
}
pub trait ParamStoreRead: ParamStore {
    fn get_owned(&self) -> <Self as ParamStore>::Type;
}
pub trait ParamStoreReadValue: ParamStore {
    fn get_value(&self) -> glib::Value;
}
pub trait ParamStoreBorrow<'a>: ParamStore {
    type BorrowType;

    fn borrow(&'a self) -> Self::BorrowType;
}
pub trait ParamStoreWrite<'a>: ParamStore {
    fn set_owned(&'a self, value: <Self as ParamStore>::Type);
    fn set_value(&'a self, value: &'a Value) {
        self.set_owned(value.get().expect("invalid value for property"));
    }
}
pub trait ParamStoreWriteChanged<'a>: ParamStoreWrite<'a> {
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool;
}

impl<T: ValueType> ParamStore for std::cell::Cell<T> {
    type Type = T;
}
impl<T> ParamStoreRead for std::cell::Cell<T>
where
    T: ValueType + Copy,
{
    fn get_owned(&self) -> <Self as ParamStore>::Type {
        std::cell::Cell::get(self)
    }
}
impl<T> ParamStoreReadValue for std::cell::Cell<T>
where
    T: ValueType + Copy,
{
    fn get_value(&self) -> glib::Value {
        self.get_owned().to_value()
    }
}
impl<'a, T> ParamStoreWrite<'a> for std::cell::Cell<T>
where
    T: ValueType,
{
    fn set_owned(&'a self, value: <Self as ParamStore>::Type) {
        self.replace(value);
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::cell::Cell<T>
where
    T: ValueType + PartialEq + Copy,
{
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
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
    fn get_owned(&self) -> <Self as ParamStore>::Type {
        self.borrow().clone()
    }
}
impl<T> ParamStoreReadValue for std::cell::RefCell<T>
where
    T: ValueType,
{
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
    fn set_owned(&'a self, value: <Self as ParamStore>::Type) {
        self.replace(value);
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::cell::RefCell<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
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
    fn get_owned(&self) -> <Self as ParamStore>::Type {
        self.borrow().clone()
    }
}
impl<T> ParamStoreReadValue for std::sync::Mutex<T>
where
    T: ValueType,
{
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
    fn set_owned(&'a self, value: <Self as ParamStore>::Type) {
        *self.lock().unwrap() = value;
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::sync::Mutex<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
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
    fn get_owned(&self) -> <Self as ParamStore>::Type {
        self.borrow().clone()
    }
}
impl<T> ParamStoreReadValue for std::sync::RwLock<T>
where
    T: ValueType,
{
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
    fn set_owned(&'a self, value: <Self as ParamStore>::Type) {
        *self.write().unwrap() = value;
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::sync::RwLock<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
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
    fn get_owned(&self) -> <Self as ParamStore>::Type {
        self.borrow().clone()
    }
}
impl<T> ParamStoreReadValue for OnceCell<T>
where
    T: ValueType,
{
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
    fn set_owned(&'a self, value: <Self as ParamStore>::Type) {
        self.set(value)
            .unwrap_or_else(|_| panic!("set() called on initialized OnceCell"));
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for OnceCell<T>
where
    T: ValueType + PartialEq + Copy,
{
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
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
    fn get_owned(&self) -> <Self as ParamStore>::Type {
        self.borrow().clone()
    }
}
impl<T> ParamStoreReadValue for SyncOnceCell<T>
where
    T: ValueType,
{
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
    fn set_owned(&'a self, value: <Self as ParamStore>::Type) {
        self.set(value)
            .unwrap_or_else(|_| panic!("set() called on initialized OnceCell"));
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for SyncOnceCell<T>
where
    T: ValueType + PartialEq + Copy,
{
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
        self.set_owned(value);
        true
    }
}

impl<T: ValueType> ParamStore for OnceBox<T> {
    type Type = T;
}
impl<T> ParamStoreRead for OnceBox<T>
where
    T: ValueType + Clone,
{
    fn get_owned(&self) -> <Self as ParamStore>::Type {
        self.borrow().clone()
    }
}
impl<T> ParamStoreReadValue for OnceBox<T>
where
    T: ValueType,
{
    fn get_value(&self) -> glib::Value {
        self.borrow().to_value()
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
    fn set_owned(&'a self, value: <Self as ParamStore>::Type) {
        self.set(Box::new(value))
            .unwrap_or_else(|_| panic!("set() called on initialized OnceBox"));
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for OnceBox<T>
where
    T: ValueType + PartialEq + Copy,
{
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
        self.set_owned(value);
        true
    }
}

impl ParamStore for OnceBool {
    type Type = bool;
}
impl ParamStoreRead for OnceBool {
    fn get_owned(&self) -> <Self as ParamStore>::Type {
        self.get()
            .unwrap_or_else(|| panic!("`get()` called on uninitialized OnceBool"))
    }
}
impl ParamStoreReadValue for OnceBool {
    fn get_value(&self) -> glib::Value {
        self.get_owned().to_value()
    }
}
impl<'a> ParamStoreWrite<'a> for OnceBool {
    fn set_owned(&'a self, value: <Self as ParamStore>::Type) {
        self.set(value)
            .unwrap_or_else(|_| panic!("set() called on initialized OnceBool"));
    }
}
impl<'a> ParamStoreWriteChanged<'a> for OnceBool {
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
        self.set_owned(value);
        true
    }
}

impl<T: ValueType> ParamStore for ConstructCell<T> {
    type Type = T;
}
impl<T> ParamStoreRead for ConstructCell<T>
where
    T: ValueType + Clone,
{
    fn get_owned(&self) -> <Self as ParamStore>::Type {
        self.borrow().clone()
    }
}
impl<T> ParamStoreReadValue for ConstructCell<T>
where
    T: ValueType,
{
    fn get_value(&self) -> glib::Value {
        self.borrow().to_value()
    }
}
impl<'a, T> ParamStoreBorrow<'a> for ConstructCell<T>
where
    T: ValueType + 'a,
{
    type BorrowType = std::cell::Ref<'a, T>;

    fn borrow(&'a self) -> Self::BorrowType {
        ConstructCell::borrow(self)
    }
}
impl<'a, T> ParamStoreWrite<'a> for ConstructCell<T>
where
    T: ValueType,
{
    fn set_owned(&'a self, value: <Self as ParamStore>::Type) {
        self.replace(value);
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for ConstructCell<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
        let mut storage = self.borrow_mut();
        let old = std::mem::replace(storage.deref_mut(), value);
        old != *storage
    }
}

macro_rules! atomic_type {
    ($ty:ty, $inner:ty) => {
        impl ParamStore for $ty {
            type Type = $inner;
        }
        impl ParamStoreRead for $ty {
            fn get_owned(&self) -> <Self as ParamStore>::Type {
                self.load(Ordering::Acquire)
            }
        }
        impl ParamStoreReadValue for $ty {
            fn get_value(&self) -> glib::Value {
                self.get_owned().to_value()
            }
        }
        impl<'a> ParamStoreWrite<'a> for $ty {
            fn set_owned(&'a self, value: <Self as ParamStore>::Type) {
                self.store(value, Ordering::Release);
            }
        }
        impl<'a> ParamStoreWriteChanged<'a> for $ty {
            fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
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
    type Type = glib::types::Pointer;
}
impl<T> ParamStoreRead for std::sync::atomic::AtomicPtr<T> {
    fn get_owned(&self) -> <Self as ParamStore>::Type {
        self.load(Ordering::Acquire) as glib::types::Pointer
    }
}
impl<T> ParamStoreReadValue for std::sync::atomic::AtomicPtr<T> {
    fn get_value(&self) -> glib::Value {
        self.get_owned().to_value()
    }
}
impl<'a, T> ParamStoreWrite<'a> for std::sync::atomic::AtomicPtr<T> {
    fn set_owned(&'a self, value: <Self as ParamStore>::Type) {
        self.store(value as *mut T, Ordering::Release);
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::sync::atomic::AtomicPtr<T> {
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
        let value = value as *mut T;
        let old = self.swap(value, Ordering::Release);
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
    fn get_owned(&self) -> <Self as ParamStore>::Type {
        unimplemented!("get() called on abstract property");
    }
}
impl<T> ParamStoreReadValue for std::marker::PhantomData<T>
where
    T: ValueType,
{
    fn get_value(&self) -> glib::Value {
        self.get_owned().to_value()
    }
}
impl<'a, T> ParamStoreWrite<'a> for std::marker::PhantomData<T>
where
    T: ValueType,
{
    fn set_owned(&'a self, _value: <Self as ParamStore>::Type) {
        unimplemented!("set() called on abstract property");
    }
}
impl<'a, T> ParamStoreWriteChanged<'a> for std::marker::PhantomData<T>
where
    T: ValueType + PartialEq,
{
    fn set_owned_checked(&'a self, value: <Self as ParamStore>::Type) -> bool {
        self.set_owned(value);
        false
    }
}
