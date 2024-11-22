use std::ops::{Deref, DerefMut};

use crate::SgRequest;

use super::UserGroup;

#[derive(Debug)]
pub struct And<A, B> {
    pub a: A,
    pub b: B,
}

impl<A, B> UserGroup for And<A, B>
where
    A: UserGroup,
    B: UserGroup,
{
    fn is_match(&self, req: &SgRequest) -> bool {
        self.a.is_match(req) && self.b.is_match(req)
    }
}

#[derive(Debug)]
pub struct Not<A> {
    pub a: A,
}

impl<A: UserGroup> UserGroup for Not<A> {
    fn is_match(&self, req: &SgRequest) -> bool {
        !self.a.is_match(req)
    }
}

#[derive(Debug)]
pub struct Or<A, B> {
    pub a: A,
    pub b: B,
}

impl<A, B> UserGroup for Or<A, B>
where
    A: UserGroup,
    B: UserGroup,
{
    fn is_match(&self, req: &SgRequest) -> bool {
        self.a.is_match(req) || self.b.is_match(req)
    }
}

pub struct All {
    pub groups: Vec<Box<dyn UserGroup>>,
}

impl std::fmt::Debug for All {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicAll").finish()
    }
}

impl Deref for All {
    type Target = Vec<Box<dyn UserGroup>>;

    fn deref(&self) -> &Self::Target {
        &self.groups
    }
}

impl DerefMut for All {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.groups
    }
}

impl UserGroup for All {
    fn is_match(&self, req: &SgRequest) -> bool {
        self.groups.iter().all(|g| g.is_match(req))
    }
}

impl All {
    pub fn new(groups: Vec<Box<dyn UserGroup>>) -> Self {
        All { groups }
    }
}

pub struct Any {
    pub groups: Vec<Box<dyn UserGroup>>,
}

impl std::fmt::Debug for Any {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicAny").finish()
    }
}

impl Deref for Any {
    type Target = Vec<Box<dyn UserGroup>>;

    fn deref(&self) -> &Self::Target {
        &self.groups
    }
}

impl DerefMut for Any {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.groups
    }
}

impl UserGroup for Any {
    fn is_match(&self, req: &SgRequest) -> bool {
        self.groups.iter().any(|g| g.is_match(req))
    }
}
