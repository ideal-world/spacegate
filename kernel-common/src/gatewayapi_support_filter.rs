use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterHeaderModifier {
    pub kind: SgFilterHeaderModifierKind,
    pub sets: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub enum SgFilterHeaderModifierKind {
    #[default]
    Request,
    Response,
}
