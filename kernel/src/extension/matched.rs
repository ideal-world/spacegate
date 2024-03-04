use crate::helper_layers::route::Router;

#[derive(Debug, Clone)]
pub struct Matched<R: Router> {
    pub router: R,
    pub index: R::Index,
}
