//! Foreign-function-interface shims for dynamic libraries we load at runtime.
//!
//! Both sub-modules use `dlopen2::wrapper_api!` to generate a type-safe
//! struct whose fields are function pointers populated when the shared library
//! is opened with `Container::load`.  No link-time dependency on either
//! library is introduced.

pub mod cuda;
pub mod libevdi;
