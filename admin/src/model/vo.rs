pub mod backend_vo;
pub mod gateway_vo;
pub mod http_route_vo;
pub mod plugin_vo;

/// Vo is a base until for admin.
/// Vo is the smallest operable and storage unit in admin.
pub trait Vo {
    /// Get vo type
    /// eg. BackendRefVO::get_vo_type() should return "BackendRef"
    fn get_vo_type() -> String;
    /// Get vo unique name
    /// unique name is used to distinguish different instances of the same type
    fn get_unique_name(&self) -> String;
}