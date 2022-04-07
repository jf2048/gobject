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
//!     //     move #[default_panic] |#[weak(self)] this| {}
//!     //
//!     // default return:
//!     //     move |#[weak(or_return)] value| {}
//!     //     move |#[weak(or_return 123)] value| {}
//!     //     move |#[weak(self or_return)] this| {}
//!     //     move |#[weak(self or_return 123)] this| {}
//!     //     move #[default_return] |#[weak(self)] this| {}
//!     //     move #[default_return 123] |#[weak(self)] this| {}
//!     //
//!     // default alternative:
//!     //     move |#[weak(or 123)] value| {}
//!     //     move |#[weak(self.myvalue or 123)] value| {}
//!     //
//!     // forcing an Option when another default is present:
//!     //     move #[default_panic] |#[weak(self)] this, #[weak(allow_none)] value| {}
//!     //     move #[default_panic] |#[weak(self)] this, #[weak(self.myvalue allow_none)] value| {}
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

#[cfg(feature = "gtk4_macros")]
pub use gobject_macros::gtk4_widget;
pub use gobject_macros::{class, clone_block, interface, Properties};

mod buildable;
pub use buildable::*;
mod construct_cell;
pub use construct_cell::*;
mod store;
pub use store::*;

pub use glib::once_cell::race::{OnceBool, OnceBox};
pub use glib::once_cell::sync::OnceCell as SyncOnceCell;
pub use glib::once_cell::unsync::OnceCell;
