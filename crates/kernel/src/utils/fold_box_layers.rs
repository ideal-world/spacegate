use crate::{ArcHyperService, BoxLayer};

pub fn fold_layers<'a>(layers: impl Iterator<Item = &'a BoxLayer> + std::iter::DoubleEndedIterator, mut inner: ArcHyperService) -> ArcHyperService {
    for l in layers.rev() {
        inner = l.layer_boxed(inner);
    }
    inner
}
