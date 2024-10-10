use std::ops::Deref;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct IsEastWestTraffic(bool);

impl IsEastWestTraffic {
    pub fn new(is_east_west: bool) -> Self {
        Self(is_east_west)
    }
}

impl Deref for IsEastWestTraffic {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
