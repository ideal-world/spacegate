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
