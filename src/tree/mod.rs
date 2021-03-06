//! Contains functionality for dispatching methods on a D-Bus "server".
//! Supersedes the `obj` module.
//!
//! # Example
//! ```rust,no_run
//! use dbus::{tree, Connection, BusType};
//! let f = tree::Factory::new_fn::<()>();
//! /* Add a method returning "Thanks!" on interface "com.example.dbus.rs"
//!    on object path "/example". */
//! let t = f.tree(()).add(f.object_path("/example", ()).introspectable()
//!     .add(f.interface("com.example.dbus.rs", ())
//!         .add_m(f.method("CallMe", (), |m| {
//!             Ok(vec!(m.msg.method_return().append("Thanks!"))) }
//!         ).out_arg("s"))
//! ));
//!
//! let c = Connection::get_private(BusType::Session).unwrap();
//! t.set_registered(&c, true).unwrap();
//! /* Run forever */
//! for _ in t.run(&c, c.iter(1000)) {}
//! ```
//!
//! See `examples/server.rs` and `examples/adv_server.rs` for more thorough examples.

mod utils;
mod methodtype;
mod leaves;
mod objectpath;
mod factory;

pub use self::utils::Argument;
pub use self::methodtype::{MethodErr, MethodInfo, PropInfo, MethodResult, MethodType, DataType, MTFn, MTFnMut, MTSync};
pub use self::leaves::{Method, Signal, Property, Access, EmitsChangedSignal};
pub use self::objectpath::{Interface, ObjectPath, Tree, TreeServer};
pub use self::factory::Factory;
