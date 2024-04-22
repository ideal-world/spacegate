use std::ffi::OsStr;

use spacegate_kernel::BoxResult;

/// Macro to register plugins from a dynamic library.
///
/// # Usage
/// ```rust no_run
/// use spacegate_plugin::dynamic_lib;
/// dynamic_lib! {
///     #[cfg(feature = "my_plugin1")]
///     MyPlugin1,
///     MyPlugin2,
///     MyPlugin3,
/// }
///
/// ```
#[macro_export]
macro_rules! dynamic_lib {
    ($(
        $(#[$m:meta])*
        $Type:ty
    ),*) => {
        #[no_mangle]
        pub extern "Rust" fn register(repo: &$crate::SgPluginRepository) {
            $(
                $(#[$m])*
                repo.register::<$Type>();
            )*
        }
    };
}
impl crate::SgPluginRepository {
    ///
    /// # Usage
    /// The library must implement a function named `register` with the following signature:
    /// ```rust no_run
    /// pub extern "Rust" fn register(repo: &SgPluginRepository) {
    ///     ...
    /// }
    /// ```
    /// A way to define this function is using the [`crate::dynamic_lib!`] macro.
    ///
    /// # Safety
    /// Loading a dynamic library could lead to undefined behavior if the library is not implemented correctly.
    ///
    /// Loaded libraries will be leaked and never unloaded, so you should be careful with this function.
    ///
    /// # Errors
    /// Target is not a valid dynamic library or the library does not implement the `register` function.
    pub unsafe fn register_lib<P: AsRef<OsStr>>(&self, path: P) -> BoxResult<()> {
        let lib = libloading::Library::new(path)?;
        let register: libloading::Symbol<unsafe extern "Rust" fn(&crate::SgPluginRepository)> = lib.get(b"register")?;
        register(self);
        let lib = Box::new(lib);
        Box::leak(lib);
        Ok(())
    }
}
