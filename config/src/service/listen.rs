use std::error::Error;

mod fs;

pub enum ConfigEventType {
    Create,
    Update,
    Delete,
}

pub enum ConfigType {
    Gateway { name: String },
    Route { gateway_name: String, name: String },
}

pub trait CreateListener {
    const CONFIG_LISTENER_NAME: &'static str;
    fn create_listener(&self) -> Result<Box<dyn Listen>, Box<dyn Error + Sync + Send + 'static>>;
}

pub trait Listen: Unpin {
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<(ConfigType, ConfigEventType)>>;
}
