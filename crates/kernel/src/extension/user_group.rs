pub mod combinator;
pub mod implementation;

use crate::SgRequest;

pub trait UserGroup {
    fn is_match(&self, req: &SgRequest) -> bool;
}

pub trait UserGroupExt: UserGroup {
    fn and<B>(self, b: B) -> combinator::And<Self, B>
    where
        Self: Sized,
    {
        combinator::And { a: self, b }
    }

    fn or<B>(self, b: B) -> combinator::Or<Self, B>
    where
        Self: Sized,
    {
        combinator::Or { a: self, b }
    }

    fn not(self) -> combinator::Not<Self>
    where
        Self: Sized,
    {
        combinator::Not { a: self }
    }

    fn boxed(self) -> Box<dyn UserGroup>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

impl<T: UserGroup> UserGroupExt for T {}
