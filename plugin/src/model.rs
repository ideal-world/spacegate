use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpPathModifier {
    /// Type defines the type of path modifier.
    pub kind: SgHttpPathModifierType,
    /// Value is the value to be used to replace the path during forwarding.
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
                let mut path_segments = path.split('/').filter(|s| !s.is_empty());
                let mut prefix_segments = prefix_match.split('/').filter(|s| !s.is_empty());
                let mut new_path = vec![""];
                loop {
                    match (path_segments.next(), prefix_segments.next()) {
                        (Some(path_seg), Some(prefix_seg)) => {
                            if !path_seg.eq_ignore_ascii_case(prefix_seg) {
                                return None;
                            }
                            new_path.push(path_seg)
                        }
                        (rest_path, None) => {
                            new_path.extend(rest_path);
                            new_path.extend(path_segments);
                            return Some(new_path.join("/"));
                        }
                        (None, Some(_)) => return None,
                    }
                }
            }
        }
    }
}
