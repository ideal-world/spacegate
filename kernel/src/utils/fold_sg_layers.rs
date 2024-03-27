use tower_layer::Layer;

use crate::{BoxHyperService, SgBoxLayer};

pub fn sg_layers<'a>(layers: impl Iterator<Item = &'a SgBoxLayer>, mut inner: BoxHyperService) -> BoxHyperService {
    for l in layers {
        inner = l.layer(inner);
    }
    inner
}
