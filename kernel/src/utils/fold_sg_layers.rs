use tower_layer::Layer;

use crate::{ArcHyperService, SgBoxLayer};

pub fn sg_layers<'a>(layers: impl Iterator<Item = &'a SgBoxLayer> + DoubleEndedIterator, mut inner: ArcHyperService) -> ArcHyperService {
    for l in layers.rev() {
        inner = l.layer(inner);
    }
    inner
}
