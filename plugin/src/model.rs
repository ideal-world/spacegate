use serde::{Deserialize, Serialize};
use spacegate_kernel::layers::http_route::match_request::SgHttpPathMatch;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SgHttpPathModifier {
    /// Type defines the type of path modifier.
    pub kind: SgHttpPathModifierType,
    /// Value is the value to be used to replace the path during forwarding.
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default, Copy)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub enum SgHttpPathModifierType {
    /// This type of modifier indicates that the full path will be replaced by the specified value.
    ReplaceFullPath,
    /// This type of modifier indicates that any prefix path matches will be replaced by the substitution value.
    /// For example, a path with a prefix match of “/foo” and a ReplacePrefixMatch substitution of “/bar” will have the “/foo” prefix replaced with “/bar” in matching requests.
    #[default]
    ReplacePrefixMatch,
    ReplaceRegex,
}

impl SgHttpPathModifier {
    pub fn replace(&self, path: &str, path_match: &SgHttpPathMatch) -> Option<String> {
        let value = &self.value;
        match (self.kind, path_match) {
            (SgHttpPathModifierType::ReplaceFullPath, _) => {
                if value.eq_ignore_ascii_case(path) {
                    Some(value.clone())
                } else {
                    None
                }
            }
            (SgHttpPathModifierType::ReplacePrefixMatch, SgHttpPathMatch::Prefix(prefix)) => {
                fn not_empty(s: &&str) -> bool {
                    !s.is_empty()
                }
                let mut path_segments = path.split('/').filter(not_empty);
                let mut prefix_segments = prefix.split('/').filter(not_empty);
                loop {
                    match (path_segments.next(), prefix_segments.next()) {
                        (Some(path_seg), Some(prefix_seg)) => {
                            if !path_seg.eq_ignore_ascii_case(prefix_seg) {
                                return None;
                            }
                        }
                        (None, None) => {
                            // handle with duplicated stash and no stash
                            let mut new_path = String::from("/");
                            new_path.push_str(self.value.trim_start_matches('/'));
                            return Some(new_path);
                        }
                        (Some(rest_path), None) => {
                            let mut new_path = String::from("/");
                            let replace_value = self.value.trim_matches('/');
                            new_path.push_str(replace_value);
                            if !replace_value.is_empty() {
                                new_path.push('/');
                            }
                            new_path.push_str(rest_path);
                            for seg in path_segments {
                                new_path.push('/');
                                new_path.push_str(seg);
                            }
                            if path.ends_with('/') {
                                new_path.push('/')
                            }
                            return Some(new_path);
                        }
                        (None, Some(_)) => return None,
                    }
                }
            }
            (SgHttpPathModifierType::ReplaceRegex, SgHttpPathMatch::Regular(re)) => {
                Some(re.replace(path, value).to_string())
            },
            _ => None
        }
    }
}

#[test]
fn test_prefix_replace() {
    let modifier = SgHttpPathModifier {
        kind: SgHttpPathModifierType::ReplacePrefixMatch,
        value: "/iam".into(),
    };
    let replace = SgHttpPathMatch::Prefix("api/iam".into());
    assert_eq!(Some("/iam/get_name"), modifier.replace("api/iam/get_name", &replace).as_deref());
    assert_eq!(
        Some("/iam/get_name/example.js"),
        modifier.replace("api/iam/get_name/example.js", &replace).as_deref()
    );
    assert_eq!(Some("/iam/get_name/"), modifier.replace("api/iam/get_name/", &replace).as_deref());
}

#[test]
fn test_regex_replace() {
    let modifier = SgHttpPathModifier {
        kind: SgHttpPathModifierType::ReplaceRegex,
        value: "/path/$1/subpath$2".into(),
    };
    let replace = SgHttpPathMatch::Regular(regex::Regex::new(r"/api/(\w*)/subpath($|/.*)").expect("invalid regex"));
    assert_eq!(Some("/path/iam/subpath/get_name"), modifier.replace("/api/iam/subpath/get_name", &replace).as_deref());
    assert_eq!(Some("/path/iam/subpath/"), modifier.replace("/api/iam/subpath/", &replace).as_deref());
    assert_eq!(Some("/path/iam/subpath"), modifier.replace("/api/iam/subpath", &replace).as_deref());
    // won't match
    assert_eq!(Some("/api/iam/subpath2"), modifier.replace("/api/iam/subpath2", &replace).as_deref());
}
