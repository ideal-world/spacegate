// use spacegate_plugin::{plugins, Plugin, PluginSchemaExt};
// use tardis::serde_json;

// fn export_plugin<P: PluginSchemaExt + Plugin>(dir: std::path::PathBuf) {
//     let schema = P::schema();
//     let json = serde_json::to_string_pretty(&schema).unwrap();
//     let filename = format!("{}.schema.json", P::CODE);
//     let path = dir.join(filename);
//     std::fs::write(path, json).unwrap();
// }

// macro_rules! export_plugins {
//     ($path: literal : $($plugin:ty)*) => {
//         let dir = std::path::PathBuf::from($path);
//         std::fs::create_dir_all(&dir).unwrap();
//         $(export_plugin::<$plugin>(dir.clone());)*
//     };
// }

// #[test]
// fn export_schema() {
//     use plugins::{
//         header_modifier::HeaderModifierPlugin, inject::InjectPlugin, limit::RateLimitPlugin, maintenance::MaintenancePlugin, redirect::RedirectPlugin, retry::RetryPlugin,
//         rewrite::RewritePlugin, status::SgFilterStatusConfig,
//     };
//     export_plugins!("schema":
//         HeaderModifierPlugin
//         InjectPlugin
//         RateLimitPlugin
//         MaintenancePlugin
//         RedirectPlugin
//         RetryPlugin
//         RewritePlugin
//         SgFilterStatusConfig

//     );
// }
