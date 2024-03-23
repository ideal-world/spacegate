use std::sync::Arc;

use spacegate_kernel::layers::gateway::SgGatewayLayer;

use crate::PluginInstance;

pub trait MountPoint {
    fn name(&self) -> String;
    fn mount(self: Arc<Self>, instance: PluginInstance);
}

impl MountPoint for SgGatewayLayer {
    fn name(&self) -> String {
        self.name()
    }

    fn mount(self: Arc<Self>, instance: PluginInstance) {
        
    }
}