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
    ipv4: BTreeMap<Ipv4Addr, T>,
    ipv6: BTreeMap<Ipv6Addr, T>,
    host: HostnameMatcherNode<T>,
}

impl<T> Default for HostnameTree<T> {
    fn default() -> Self {
        Self {
            ipv4: BTreeMap::new(),
            ipv6: BTreeMap::new(),
            host: HostnameMatcherNode::new(),
        }
    }
}

impl<T> HostnameTree<T> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get(&self, host: &str) -> Option<&T> {
        // trim port
        if host.starts_with('[') {
            let bracket_end = host.find(']')?;
            let ipv6 = host[1..bracket_end].parse::<Ipv6Addr>().ok()?;
            return self.ipv6.get(&ipv6);
        } else {
            let host = host.rsplit_once(':').map(|(host, _)| host).unwrap_or(host);
            if let Ok(ipv4) = host.parse::<Ipv4Addr>() {
                self.ipv4.get(&ipv4)
            } else {
                self.host.get(host)
            }
        }
    }
    pub fn set(&mut self, host: &str, data: T) {
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

/// we don't neet a radix tree here, because host name won't be too long
#[derive(Clone)]
pub struct HostnameMatcherNode<T> {
    data: Option<T>,
    children: BTreeMap<String, HostnameMatcherNode<T>>,
    /// for * match
    else_node: Option<Box<HostnameMatcherNode<T>>>,
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
    pub fn get(&self, host: &str) -> Option<&T> {
        let host = host.to_ascii_lowercase();
        self.get_by_iter(host.split('.').rev())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_hostname_matcher_node() {
        let mut tree = HostnameTree::new();
        macro_rules! test_cases {
            ($tree: ident
                $(![$($unmatched_case: literal),*])?
                $([$($case: literal),*] => $rule:literal)*
            ) => {
                $($tree.set($rule, $rule);)*
                println!("{:#?}", tree.host);
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

        test_cases! {
            tree
            !["[::1]", "127.0.0.1"]
            ["[::0]", "[::0]:80", "[::]"] => "[::0]"
            ["192.168.0.1"] => "192.168.0.1"
            ["example.com", "example.com:80"] => "example.com"
            ["api.example.com", "apL.v1.example.com:1000"] => "*.example.com"
            ["api.v1.example.com", "api.v2.example.com"] => "api.*.example.com"
            ["baidu.com"] => "*.com"
            ["com", "example.org", "example.org:80", "example.org:443", "localhost:8080"] => "*"
        }
    }
}
