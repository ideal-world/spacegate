pub mod base_dto;
pub mod query_dto;
pub mod vo;
pub mod vo_converter;
#[cfg(feature = "k8s")]
pub trait ToFields {
    fn to_fields_vec(&self) -> Vec<String>;
    fn to_fields(&self) -> String {
        self.to_fields_vec().join(",")
    }
}
