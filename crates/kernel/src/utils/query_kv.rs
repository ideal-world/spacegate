/// A zero-copy query key-value iterator.
///
/// # Example
/// ```rust
/// # use spacegate_kernel::utils::QueryKvIter;
/// # fn main() {
/// let query = "a=1&b=2&c";
/// let mut iter = QueryKvIter::new(query);
/// assert_eq!(iter.next(), Some(("a", Some("1"))));
/// assert_eq!(iter.next(), Some(("b", Some("2"))));
/// assert_eq!(iter.next(), Some(("c", None)));
/// # }
/// ```
pub struct QueryKvIter<'a> {
    inner: &'a str,
}

impl<'a> QueryKvIter<'a> {
    pub fn new(query: &'a str) -> Self {
        Self { inner: query }
    }
}

impl<'a> Iterator for QueryKvIter<'a> {
    type Item = (&'a str, Option<&'a str>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.inner.is_empty() {
            return None;
        }
        match self.inner.split_once('&') {
            Some((k, v)) => {
                self.inner = v;
                match k.split_once('=') {
                    Some((k, v)) => Some((k, Some(v))),
                    None => Some((k, None)),
                }
            }
            None => {
                let k = self.inner;
                self.inner = "";
                Some((k, None))
            }
        }
    }
}
