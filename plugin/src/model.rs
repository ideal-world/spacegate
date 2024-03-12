use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SgHttpPathModifier {
    /// Type defines the type of path modifier.
    pub kind: SgHttpPathModifierType,
    /// Value is the value to be used to replace the path during forwarding.
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub enum SgHttpPathModifierType {
    /// This type of modifier indicates that the full path will be replaced by the specified value.
    ReplaceFullPath,
    /// This type of modifier indicates that any prefix path matches will be replaced by the substitution value.
    /// For example, a path with a prefix match of “/foo” and a ReplacePrefixMatch substitution of “/bar” will have the “/foo” prefix replaced with “/bar” in matching requests.
    #[default]
    ReplacePrefixMatch,
}

impl SgHttpPathModifier {
    pub fn replace(&self, path: &str, prefix_match: Option<&str>) -> Option<String> {
        let value = &self.value;
        match self.kind {
            SgHttpPathModifierType::ReplaceFullPath => {
                if value.eq_ignore_ascii_case(path) {
                    Some(value.clone())
                } else {
                    None
                }
            }
            SgHttpPathModifierType::ReplacePrefixMatch => {
                let prefix_match = prefix_match?;
                fn not_empty(s: &&str) -> bool {
                    !s.is_empty()
                }
                let mut path_segments = path.split('/').filter(not_empty);
                let mut prefix_segments = prefix_match.split('/').filter(not_empty);
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
        }
    }
}

#[test]
fn test_replace() {
    let modifier = SgHttpPathModifier {
        kind: SgHttpPathModifierType::ReplacePrefixMatch,
        value: "/iam".into(),
    };
    assert_eq!(Some("/iam/get_name"), modifier.replace("api/iam/get_name", Some("api/iam")).as_deref());
    assert_eq!(Some("/iam/get_name/example.js"), modifier.replace("api/iam/get_name/example.js", Some("api/iam")).as_deref());
    assert_eq!(Some("/iam/get_name/"), modifier.replace("api/iam/get_name/", Some("api/iam")).as_deref());
}
