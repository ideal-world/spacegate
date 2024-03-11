//! # Match Hostnames
//!
//! ## Priority
//!
//! | Priority |   Rule             |  Example              |
//! |:--------:|:-------------------|:----------------------|
//! |  0       | Exact Host         |  example.com          |
//! |  1       | Partial wildcard   |  *.example.com        |
//! |  2       | Wild Card          |  *                    |
//!
//! it would be a tree like this:
//!
//! ```text
//! com
//! |
//! +- example
//! |  |
//! |  +- next
//! |  \- *
//! |       
//! \- *
//!
//!
//! ```

use std::{
    collections::BTreeMap,
    fmt::{self},
    net::{Ipv4Addr, Ipv6Addr},
};

#[derive(Debug, Clone)]
pub struct HostnameTree<T> {
    pub(crate) ipv4: BTreeMap<Ipv4Addr, T>,
    pub(crate) ipv6: BTreeMap<Ipv6Addr, T>,
    pub(crate) host: HostnameMatcherNode<T>,
    pub(crate) fallback: Option<T>,
}

impl<T> Default for HostnameTree<T> {
    fn default() -> Self {
        Self {
            ipv4: BTreeMap::new(),
            ipv6: BTreeMap::new(),
            host: HostnameMatcherNode::new(),
            fallback: None,
        }
    }
}

impl<T> HostnameTree<T> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn iter(&self) -> HostnameTreeIter<T> {
        HostnameTreeIter {
            ipv4: self.ipv4.values(),
            ipv6: self.ipv6.values(),
            host: self.host.iter(),
            fallback: self.fallback.iter(),
        }
    }
    pub fn iter_mut(&mut self) -> HostnameTreeIterMut<T> {
        HostnameTreeIterMut {
            ipv4: self.ipv4.values_mut(),
            ipv6: self.ipv6.values_mut(),
            host: self.host.iter_mut(),
            fallback: self.fallback.iter_mut(),
        }
    }
    pub fn get(&self, host: &str) -> Option<&T> {
        // trim port
        let data = if host.starts_with('[') {
            let bracket_end = host.find(']')?;
            let ipv6 = host[1..bracket_end].parse::<Ipv6Addr>().ok()?;
            self.ipv6.get(&ipv6)
        } else {
            let host = host.rsplit_once(':').map(|(host, _)| host).unwrap_or(host);
            if let Ok(ipv4) = host.parse::<Ipv4Addr>() {
                self.ipv4.get(&ipv4)
            } else {
                self.host.get(host)
            }
        };
        data.or(self.fallback.as_ref())
    }
    pub fn get_mut(&mut self, host: &str) -> Option<&mut T> {
        // trim port
        let data = if host.starts_with('[') {
            let bracket_end = host.find(']')?;
            let ipv6 = host[1..bracket_end].parse::<Ipv6Addr>().ok()?;
            self.ipv6.get_mut(&ipv6)
        } else {
            let host = host.rsplit_once(':').map(|(host, _)| host).unwrap_or(host);
            if let Ok(ipv4) = host.parse::<Ipv4Addr>() {
                self.ipv4.get_mut(&ipv4)
            } else {
                self.host.get_mut(host)
            }
        };
        data.or(self.fallback.as_mut())
    }
    pub fn set(&mut self, host: &str, data: T) {
        if host == "*" {
            self.fallback = Some(data);
            return;
        }
        if host.starts_with('[') {
            if let Some(ipv6) = host.strip_prefix('[').and_then(|host| host.strip_suffix(']')) {
                if let Ok(ipv6) = ipv6.parse::<Ipv6Addr>() {
                    self.ipv6.insert(ipv6, data);
                }
            }
        } else if let Ok(ipv4) = host.parse::<Ipv4Addr>() {
            self.ipv4.insert(ipv4, data);
        } else {
            self.host.set(host, data);
        }
    }
}

pub struct HostnameTreeIter<'a, T> {
    ipv4: std::collections::btree_map::Values<'a, Ipv4Addr, T>,
    ipv6: std::collections::btree_map::Values<'a, Ipv6Addr, T>,
    host: HostnameMatcherNodeIter<'a, T>,
    fallback: std::option::Iter<'a, T>,
}

impl<'a, T: 'a> Iterator for HostnameTreeIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(data) = self.ipv4.next() {
            return Some(data);
        }
        if let Some(data) = self.ipv6.next() {
            return Some(data);
        }
        if let Some(data) = self.host.next() {
            return Some(data);
        }
        self.fallback.next()
    }
}

pub struct HostnameTreeIterMut<'a, T> {
    ipv4: std::collections::btree_map::ValuesMut<'a, Ipv4Addr, T>,
    ipv6: std::collections::btree_map::ValuesMut<'a, Ipv6Addr, T>,
    host: HostnameMatcherNodeIterMut<'a, T>,
    fallback: std::option::IterMut<'a, T>,
}

impl<'a, T: 'a> Iterator for HostnameTreeIterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(data) = self.ipv4.next() {
            return Some(data);
        }
        if let Some(data) = self.ipv6.next() {
            return Some(data);
        }
        if let Some(data) = self.host.next() {
            return Some(data);
        }
        self.fallback.next()
    }
}

/// we don't neet a radix tree here, because host name won't be too long
#[derive(Clone)]
pub struct HostnameMatcherNode<T> {
    data: Option<T>,
    children: BTreeMap<String, HostnameMatcherNode<T>>,
    /// for * match
    else_node: Option<Box<HostnameMatcherNode<T>>>,
}

pub struct HostnameMatcherNodeIter<'a, T: 'a> {
    data: std::option::Iter<'a, T>,
    children: std::collections::btree_map::Values<'a, String, HostnameMatcherNode<T>>,
    else_node: Option<Box<HostnameMatcherNodeIter<'a, T>>>,
}

impl<'a, T: 'a> Iterator for HostnameMatcherNodeIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(data) = self.data.next() {
            return Some(data);
        }
        if let Some(node) = self.children.next() {
            return node.iter().next();
        }
        if let Some(node) = self.else_node.as_mut() {
            return node.next();
        }
        None
    }
}

pub struct HostnameMatcherNodeIterMut<'a, T: 'a> {
    data: std::option::IterMut<'a, T>,
    children: std::collections::btree_map::ValuesMut<'a, String, HostnameMatcherNode<T>>,
    else_node: Option<Box<HostnameMatcherNodeIterMut<'a, T>>>,
}

impl<'a, T: 'a> Iterator for HostnameMatcherNodeIterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(data) = self.data.next() {
            return Some(data);
        }
        if let Some(node) = self.children.next() {
            return node.iter_mut().next();
        }
        if let Some(node) = self.else_node.as_mut() {
            return node.next();
        }
        None
    }
}

impl<T> Default for HostnameMatcherNode<T> {
    fn default() -> Self {
        Self {
            data: None,
            children: BTreeMap::new(),
            else_node: None,
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for HostnameMatcherNode<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut df = f.debug_struct("HostnameMatcherNode");
        if let Some(data) = &self.data {
            df.field("_", data);
        }
        for (key, node) in &self.children {
            df.field(key, node);
        }
        if let Some(node) = &self.else_node {
            df.field("*", node);
        }
        df.finish()
    }
}

impl<T> HostnameMatcherNode<T> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn append_by_iter<'a, I>(&mut self, mut host: I, data: T)
    where
        I: Iterator<Item = &'a str>,
    {
        if let Some(segment) = host.next() {
            match segment {
                "*" => match self.else_node {
                    Some(ref mut node) => {
                        node.append_by_iter(host, data);
                    }
                    None => {
                        let mut node = HostnameMatcherNode::new();
                        node.append_by_iter(host, data);
                        self.else_node = Some(Box::new(node));
                    }
                },
                seg => {
                    self.children.entry(seg.to_ascii_lowercase()).or_default().append_by_iter(host, data);
                }
            }
        } else {
            self.data = Some(data);
        }
    }
    pub fn set(&mut self, host: &str, data: T) {
        self.append_by_iter(host.split('.').rev(), data);
    }
    pub fn get_by_iter<'a, I>(&self, mut host: I) -> Option<&T>
    where
        I: Iterator<Item = &'a str> + Clone,
    {
        if let Some(segment) = host.next() {
            let children_match = match self.children.get(segment) {
                Some(node) => node.get_by_iter(host.clone()),
                None => None,
            };
            match children_match {
                Some(data) => Some(data),
                None => {
                    let else_node = self.else_node.as_ref()?;
                    else_node.get_by_iter(host).or(else_node.data.as_ref())
                }
            }
        } else {
            self.data.as_ref()
        }
    }
    pub fn get_mut_by_iter<'a, 'b, I>(&'b mut self, host: I) -> Option<&'b mut T>
    where
        I: Iterator<Item = &'a str> + Clone,
    {
        // it's safe to do so because we don't have any other reference to self
        self.get_by_iter(host).map(|r| unsafe {
            let r = r as *const T as *mut T;
            r.as_mut().expect("fail to convert ptr")
        })
    }
    pub fn get(&self, host: &str) -> Option<&T> {
        let host = host.to_ascii_lowercase();
        self.get_by_iter(host.split('.').rev())
    }
    pub fn get_mut(&mut self, host: &str) -> Option<&mut T> {
        let host = host.to_ascii_lowercase();
        self.get_mut_by_iter(host.split('.').rev())
    }
    pub fn iter(&self) -> HostnameMatcherNodeIter<'_, T> {
        HostnameMatcherNodeIter {
            data: self.data.iter(),
            children: self.children.values(),
            else_node: self.else_node.as_ref().map(|node| Box::new(node.iter())),
        }
    }
    pub fn iter_mut(&mut self) -> HostnameMatcherNodeIterMut<'_, T> {
        HostnameMatcherNodeIterMut {
            data: self.data.iter_mut(),
            children: self.children.values_mut(),
            else_node: self.else_node.as_mut().map(|node| Box::new(node.iter_mut())),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    macro_rules! test_cases {
        ($tree: ident
            $(![$($unmatched_case: literal),*])?
            $([$($case: literal),*] => $rule:literal)*
        ) => {
            $($tree.set($rule, $rule);)*
            println!("{:#?}", $tree.host);
            $(
                $(
                    assert_eq!($tree.get($unmatched_case), None);
                )*
            )?
            $(
                $(
                    assert_eq!($tree.get($case).cloned(), Some($rule));
                )*
            )*
        };
    }
    #[test]
    fn test_hostname_matcher_without_fallback() {
        let mut tree = HostnameTree::new();
        test_cases! {
            tree
            !["com", "127.0.0.23"]
            ["[::0]", "[::0]:80", "[::]"] => "[::0]"
            ["192.168.0.1"] => "192.168.0.1"
            ["example.com", "example.com:80"] => "example.com"
            ["api.example.com", "apL.v1.example.com:1000"] => "*.example.com"
            ["api.v1.example.com", "api.v2.example.com"] => "api.*.example.com"
            ["baidu.com"] => "*.com"
        }
    }
    #[test]
    fn test_hostname_matcher_node() {
        let mut tree = HostnameTree::new();
        test_cases! {
            tree
            ["[::0]", "[::0]:80", "[::]"] => "[::0]"
            ["192.168.0.1"] => "192.168.0.1"
            ["example.com", "example.com:80"] => "example.com"
            ["api.example.com", "apL.v1.example.com:1000"] => "*.example.com"
            ["api.v1.example.com", "api.v2.example.com"] => "api.*.example.com"
            ["baidu.com"] => "*.com"
            ["[::1]", "127.0.0.1", "com", "example.org", "example.org:80", "example.org:443", "localhost:8080"] => "*"
        }
    }
    #[test]
    fn test_any_match() {
        let mut tree = HostnameTree::new();
        test_cases! {
            tree
            ["com", "example.org", "example.org:80", "example.org:443", "localhost:8080", "127.0.0.1:9090"] => "*"
        }
    }
}
