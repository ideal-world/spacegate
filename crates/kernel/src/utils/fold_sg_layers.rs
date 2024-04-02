use tower_layer::Layer;

use crate::{ArcHyperService, SgBoxLayer};

pub fn sg_layers<'a>(layers: impl Iterator<Item = &'a SgBoxLayer>, mut inner: ArcHyperService) -> ArcHyperService {
    for l in layers {
        inner = l.layer(inner);
    }
    inner
}
