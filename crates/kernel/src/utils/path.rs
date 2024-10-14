#[derive(Debug)]
pub struct PathIter<'a> {
    inner: std::str::Split<'a, char>,
}

impl<'a> PathIter<'a> {
    pub fn new(path: &'a str) -> Self {
        Self {
            inner: path.trim_start_matches('/').split('/'),
        }
    }
}

impl<'a> Iterator for PathIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}
