use tower_layer::{Identity, Stack};

use crate::SgBoxLayer;

/// fold a bunch of layers into a single layer, the front layers will be inner layers
pub fn fold_sg_layers(layers: impl Iterator<Item = SgBoxLayer>) -> SgBoxLayer {
    layers.fold(SgBoxLayer::new(Identity::default()), |inner, outer| SgBoxLayer::new(Stack::new(inner, outer)))
}
