use std::{borrow::Cow, collections::HashMap, ffi::OsStr};

use spacegate_kernel::BoxResult;

use crate::PluginDefinitionObject;

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
    /// # Safety
    /// Loading a dynamic library could lead to undefined behavior if the library is not implemented correctly.
    ///
    /// # Usage
    /// The library must implement a function named `register` with the following signature:
    /// ```rust no_run
    /// pub extern "Rust" fn register(repo: *const SgPluginRepository);
    /// ```
    ///
    /// A way to define this function is using the [`crate::dynamic_lib!`] macro.
    pub unsafe fn register_lib(&self, path: impl AsRef<OsStr>) -> BoxResult<()> {
        let lib = libloading::Library::new(path.as_ref())?;
        let register: libloading::Symbol<unsafe extern "Rust" fn() -> &'static [(&'static str, fn() -> PluginDefinitionObject)]> = lib.get(b"register_fn_list")?;

        let mut wg = self.plugins.write().expect("fail to get write lock");
        let list = register();
        dbg!(&list);
        for (name, create) in list {
            let (name, obj) = dbg!(name, create());
            wg.insert(name.to_string().clone(), obj);
        }
        dbg!(wg);
        lib.close()?;
        Ok(())
    }
}
