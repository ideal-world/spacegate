use crate::{ArcHyperService, BoxLayer};

/// Fold layers into a single service,
/// the order of the layers is reversed.
pub fn fold_layers<'a>(layers: impl std::iter::DoubleEndedIterator<Item = &'a BoxLayer>, mut inner: ArcHyperService) -> ArcHyperService {
    for l in layers.rev() {
        inner = l.layer_shared(inner);
    }
    inner
}
