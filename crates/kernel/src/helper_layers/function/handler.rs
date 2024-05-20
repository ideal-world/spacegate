#![allow(non_snake_case)]
use futures_util::Future;
use hyper::{Request, Response};

use crate::SgBody;

use super::Inner;
use crate::extractor::Extract;
// F: Fn(Request<SgBody>, Inner) -> Fut + Send + Sync + Clone + 'static,
// Fut: Future<Output = Response<SgBody>> + Send + 'static,
pub trait HandlerFn<T, Fut>
where
    Fut: Future<Output = Response<SgBody>> + Send + 'static,
{
    fn apply(&self, request: Request<SgBody>, inner: Inner) -> Fut;
}

macro_rules! impl_handler {
    ($($t:ident),*) => {
        #[allow(unused_variables, unused_parens)]
        impl<F, Fut, $($t),* > HandlerFn<($($t),*), Fut> for F
        where
            F: (Fn(Request<SgBody>, Inner, $($t,)*) -> Fut) + Send + Sync + Clone + 'static ,
            Fut: Future<Output = Response<SgBody>> + Send + 'static,
            $($t: Extract),*
        {
            fn apply(&self, request: Request<SgBody>, inner: Inner) -> Fut {
                $(let $t = $t::extract(&request);)*
                (self.clone())(request, inner, $($t,)*)
            }
        }
    };
}

impl Inner {
    pub fn invoke<T, Fut, H>(self, request: Request<SgBody>, handler: H) -> Fut
    where
        H: HandlerFn<T, Fut>,
        Fut: Future<Output = Response<SgBody>> + Send + 'static,
    {
        handler.apply(request, self)
    }
}

impl_handler!();
impl_handler!(T0);
impl_handler!(T0, T1);
impl_handler!(T0, T1, T2);
impl_handler!(T0, T1, T2, T3);
impl_handler!(T0, T1, T2, T3, T4);
impl_handler!(T0, T1, T2, T3, T4, T5);
impl_handler!(T0, T1, T2, T3, T4, T5, T6);
impl_handler!(T0, T1, T2, T3, T4, T5, T6, T7);
impl_handler!(T0, T1, T2, T3, T4, T5, T6, T7, T8);
impl_handler!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_handler!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_handler!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_handler!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
impl_handler!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13);
impl_handler!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14);
impl_handler!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15);
