use std::{
    future::Future,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::{self, Pin},
    task::{ready, Poll},
};

pub trait Upload<D> {
    type Error;

    fn upload(&self, resource: D) -> Result<(), Self::Error>;
}

pub trait Download<D> {
    type Error;

    fn download(&self) -> Result<D, Self::Error>;
}

pub trait Listen<E> {
    type Error;

    fn poll_event(&self, cx: &std::task::Context<'_>) -> Poll<Result<Option<E>, Self::Error>>;
}

