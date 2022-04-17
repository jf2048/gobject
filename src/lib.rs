//! ### Rust GObject experiments
//!
//! ## `class` macro
//!
//! ```
//! #[gobject::class(final)]
//! mod obj {
//!     #[derive(Default)]
//!     pub struct MyObj {
//!         #[property(get, set)]
//!         my_prop: std::cell::Cell<u64>,
//!     }
//!     impl MyObj {
//!         #[signal]
//!         fn abc(&self) {}
//!     }
//! }
//!
//! # fn main() {
//!     let obj: MyObj = glib::Object::new(&[]).unwrap();
//!     obj.set_my_prop(52);
//!     obj.emit_abc();
//! # }
//! ```
//!
//! ## `clone_block` macro
//!
//! ```
//! #[gobject::clone_block]
//! fn myfunc() {
//!     use glib::prelude::ObjectExt;
//!
//!     let get_cell = {
//!         let cell = std::rc::Rc::new(std::cell::Cell::new(50u32));
//!
//!         // equivalent to glib_clone!(@weak-allow-none cell => ...)
//!         let get_cell = move |#[weak] cell| cell.map(|c| c.get()).unwrap_or(0);
//!         cell.set(100);
//!
//!         // arguments marked with #[weak] or #[strong] are passed implicitly
//!         assert_eq!(get_cell(), 100u32);
//!         get_cell
//!     };
//!     assert_eq!(get_cell(), 0u32);
//!
//!     let concat = {
//!         let refcell = std::rc::Rc::new(std::cell::RefCell::new(String::from("Hello")));
//!         let obj: glib::Object = glib::Object::new(&[]).unwrap();
//!         let concat = move |#[strong] refcell, #[strong] obj, extra: &str| {
//!             format!("{} {} {}", refcell.borrow(), obj.type_().name(), extra)
//!         };
//!         assert_eq!(concat("World"), "Hello GObject World");
//!         refcell.replace(String::from("Goodbye"));
//!         concat
//!     };
//!     assert_eq!(concat("World"), "Goodbye GObject World");
//!
//!     // other supported options
//!
//!     // renaming:
//!     //     move |#[weak(self)] this| {}
//!     //     move |#[strong(self.mydata)] this| {}
//!     //
//!     // default panic:
//!     //     move |#[weak(or_panic)] value| {}
//!     //     move |#[weak(self or_panic)] this| {}
//!     //     #[default_panic] move |#[weak(self)] this| {}
//!     //
//!     // default return:
//!     //     move |#[weak(or_return)] value| {}
//!     //     move |#[weak(or_return 123)] value| {}
//!     //     move |#[weak(self or_return)] this| {}
//!     //     move |#[weak(self or_return 123)] this| {}
//!     //     #[default_return] move |#[weak(self)] this| {}
//!     //     #[default_return 123] move |#[weak(self)] this| {}
//!     //
//!     // default alternative:
//!     //     move |#[weak(or 123)] value| {}
//!     //     move |#[weak(self.myvalue or 123)] value| {}
//!     //
//!     // forcing an Option when another default is present:
//!     //     #[default_panic] move |#[weak(self)] this, #[weak(allow_none)] value| {}
//!     //     #[default_panic] move |#[weak(self)] this, #[weak(self.myvalue allow_none)] value| {}
//!
//!     // equivalent to glib::closure!
//!     let add = #[closure] |a: i32, b: i32| a + b;
//!     assert_eq!(add.invoke::<i32>(&[&3i32, &7i32]), 10);
//!
//!     let obj: glib::Object = glib::Object::new(&[]).unwrap();
//!
//!     // equivalent to glib::closure_local!
//!     let closure = move |#[watch] obj| obj.type_().name().to_owned();
//!     assert_eq!(closure.invoke::<String>(&[]), "GObject");
//!
//!     // strong and weak references work with closures too
//!     let get_cell = {
//!         let cell = std::rc::Rc::new(std::cell::Cell::new(50u32));
//!         let get_cell = #[closure(local)] move |#[weak] cell| cell.map(|c| c.get()).unwrap_or(0);
//!         cell.set(100);
//!         assert_eq!(get_cell.invoke::<u32>(&[]), 100);
//!         get_cell
//!     };
//!     assert_eq!(get_cell.invoke::<u32>(&[]), 0);
//! }
//!
//! # myfunc();
//! ```

#[doc(hidden)]
#[cfg(feature = "gio_macros")]
pub use gio;
#[doc(hidden)]
pub use glib;
#[doc(hidden)]
#[cfg(feature = "gtk4_macros")]
pub use gtk4;
#[doc(hidden)]
#[cfg(feature = "serde_macros")]
pub use serde;

#[cfg(feature = "gtk4_macros")]
pub use gobject_macros::gtk4_widget;
#[cfg(feature = "serde_macros")]
pub use gobject_macros::serde_cast;
pub use gobject_macros::{class, clone_block, interface, Properties};

mod buildable;
pub use buildable::*;
mod construct_cell;
pub use construct_cell::*;
mod store;
pub use store::*;
mod weak_cell;
pub use weak_cell::*;
#[cfg(feature = "serde_macros")]
mod serde_traits;
#[cfg(feature = "serde_macros")]
pub use serde_traits::*;

pub use glib::once_cell::race::{OnceBool, OnceBox};
pub use glib::once_cell::sync::OnceCell as SyncOnceCell;
pub use glib::once_cell::unsync::OnceCell;

use glib::Closure;
use glib::ObjectType;
use std::ptr::NonNull;

// Helper struct to avoid creating an extra ref on objects inside closure watches. This is safe
// because `watch_closure` ensures the object has a ref when the closure is called.
#[doc(hidden)]
pub struct WatchedObject<T: ObjectType>(NonNull<T::GlibType>);

#[doc(hidden)]
unsafe impl<T: ObjectType + Send + Sync> Send for WatchedObject<T> {}

#[doc(hidden)]
unsafe impl<T: ObjectType + Send + Sync> Sync for WatchedObject<T> {}

#[doc(hidden)]
impl<T: ObjectType> WatchedObject<T> {
    pub fn new(obj: &T) -> Self {
        Self(unsafe { NonNull::new_unchecked(obj.as_ptr()) })
    }
    pub unsafe fn borrow(&self) -> glib::translate::Borrowed<T>
    where
        T: glib::translate::FromGlibPtrBorrow<*mut <T as ObjectType>::GlibType>,
    {
        glib::translate::from_glib_borrow(self.0.as_ptr())
    }
}

#[doc(hidden)]
pub trait Watchable<T: ObjectType> {
    fn watched_object(&self) -> WatchedObject<T>;
    fn watch_closure(&self, closure: &impl AsRef<Closure>);
}

#[doc(hidden)]
impl<T: ObjectType> Watchable<T> for T {
    fn watched_object(&self) -> WatchedObject<T> {
        WatchedObject::new(self)
    }
    fn watch_closure(&self, closure: &impl AsRef<Closure>) {
        glib::ObjectExt::watch_closure(self, closure)
    }
}

#[doc(hidden)]
impl<T: ObjectType> Watchable<T> for &T {
    fn watched_object(&self) -> WatchedObject<T> {
        WatchedObject::new(*self)
    }
    fn watch_closure(&self, closure: &impl AsRef<Closure>) {
        glib::ObjectExt::watch_closure(*self, closure)
    }
}
