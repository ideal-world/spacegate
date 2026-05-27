use spacegate_plugin::PluginRepository;

#[test]
fn full_feature_does_not_register_hai_plugins() {
    let repo = PluginRepository::new();
    repo.register_prelude();

    let plugin_codes = repo.plugin_list().into_iter().map(|plugin| plugin.code.to_string()).collect::<Vec<_>>();
    for hai_code in ["hai-observe", "hai-auth", "hai-asset", "hai-quota", "hai-dispatch"] {
        assert!(!plugin_codes.iter().any(|code| code == hai_code), "{hai_code} should be maintained outside SpaceGate");
    }
}
