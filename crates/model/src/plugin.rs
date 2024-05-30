use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::Display,
    hash::Hash,
    ops::{Deref, DerefMut},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::BoxError;

pub mod gatewayapi_support_filter;

#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(tag = "kind", content = "content", rename_all = "lowercase")]
pub enum PluginInstanceName {
    Anon {
        uid: String,
    },
    Named {
        /// name should be unique within the plugin code, composed of alphanumeric characters and hyphens
        name: String,
    },
    Mono,
}

impl PluginInstanceName {
    pub fn named(name: impl Into<String>) -> Self {
        PluginInstanceName::Named { name: name.into() }
    }
    pub fn mono() -> Self {
        PluginInstanceName::Mono {}
    }
    pub fn anon(uid: impl ToString) -> Self {
        PluginInstanceName::Anon { uid: uid.to_string() }
    }

    pub fn to_raw_str(&self) -> String {
        match self {
            PluginInstanceName::Anon { uid } => uid.to_string(),
            PluginInstanceName::Named { name } => name.to_string(),
            PluginInstanceName::Mono => "".to_string(),
        }
    }
}

impl From<Option<String>> for PluginInstanceName {
    fn from(value: Option<String>) -> Self {
        match value {
            Some(name) => PluginInstanceName::Named { name },
            None => PluginInstanceName::Mono,
        }
    }
}

impl From<String> for PluginInstanceName {
    fn from(value: String) -> Self {
        Some(value).into()
    }
}

#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct PluginInstanceId {
    pub code: Cow<'static, str>,
    // #[serde(flatten)]
    pub name: PluginInstanceName,
}

impl ToString for PluginInstanceId {
    fn to_string(&self) -> String {
        format!("{}-{}", self.code, self.name)
    }
}

impl From<PluginConfig> for PluginInstanceId {
    fn from(value: PluginConfig) -> Self {
        value.id
    }
}

impl PluginInstanceId {
    pub fn new(code: impl Into<Cow<'static, str>>, name: PluginInstanceName) -> Self {
        PluginInstanceId { code: code.into(), name }
    }
    pub fn parse_by_code(code: impl Into<Cow<'static, str>>, id: &str) -> Result<Self, BoxError> {
        let code = code.into();
        let name = id.strip_prefix(code.as_ref()).ok_or("unmatched code")?.trim_matches('-').parse()?;
        Ok(PluginInstanceId { code, name })
    }
    pub fn as_file_stem(&self) -> String {
        match &self.name {
            PluginInstanceName::Anon { uid } => format!("{}.{}", self.code, uid),
            PluginInstanceName::Named { ref name } => format!("{}.{}", self.code, name),
            PluginInstanceName::Mono => self.code.to_string(),
        }
    }
    pub fn from_file_stem(stem: &str) -> Self {
        let mut iter = stem.split('.');
        let code = iter.next().expect("should have the first part").to_string();
        if let Some(name) = iter.next() {
            Self::new(code, PluginInstanceName::named(name))
        } else {
            Self::new(code, PluginInstanceName::mono())
        }
    }
}

impl Display for PluginInstanceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            PluginInstanceName::Anon { uid } => write!(f, "a-{}", uid),
            PluginInstanceName::Named { name } => write!(f, "n-{}", name),
            PluginInstanceName::Mono => write!(f, "m"),
        }
    }
}

impl FromStr for PluginInstanceName {
    type Err = BoxError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("empty string".into());
        }
        let mut parts = s.splitn(2, '-');
        match parts.next() {
            Some("a") => {
                let uid = parts.next().ok_or("missing uid")?;
                Ok(PluginInstanceName::Anon { uid: uid.to_string() })
            }
            Some("n") => {
                let name = parts.next().ok_or("missing name")?;
                if name.is_empty() {
                    return Err("empty name".into());
                }
                Ok(PluginInstanceName::Named { name: name.into() })
            }
            Some("g") => Ok(PluginInstanceName::Mono {}),
            _ => Err("invalid prefix".into()),
        }
    }
}

#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct PluginConfig {
    #[serde(flatten)]
    pub id: PluginInstanceId,
    pub spec: Value,
}

impl PluginConfig {
    pub fn new(id: impl Into<PluginInstanceId>, spec: Value) -> Self {
        Self { id: id.into(), spec }
    }
    pub fn code(&self) -> &str {
        &self.id.code
    }
    pub fn name(&self) -> &PluginInstanceName {
        &self.id.name
    }
}

impl Hash for PluginConfig {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.spec.to_string().hash(state);
    }
}

#[derive(Debug, Clone, Default)]
pub struct PluginInstanceMap {
    // #[cfg_attr(feature = "typegen", ts(type = "Record<string, PluginConfig>"))]
    plugins: HashMap<PluginInstanceId, Value>,
}

impl PluginInstanceMap {
    pub fn into_config_vec(self) -> Vec<PluginConfig> {
        self.plugins.into_iter().map(|(k, v)| PluginConfig { id: k, spec: v }).collect()
    }
    pub fn from_config_vec(vec: Vec<PluginConfig>) -> Self {
        let map = vec.into_iter().map(|v| (v.id.clone(), v.spec)).collect();
        PluginInstanceMap { plugins: map }
    }
}

#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, rename = "PluginInstanceMap"))]
#[allow(dead_code)]
pub(crate) struct PluginInstanceMapTs {
    #[allow(dead_code)]
    plugins: HashMap<String, PluginConfig>,
}

impl PluginInstanceMap {
    pub fn new(plugins: HashMap<PluginInstanceId, Value>) -> Self {
        PluginInstanceMap { plugins }
    }
    pub fn into_inner(self) -> HashMap<PluginInstanceId, Value> {
        self.plugins
    }
}

impl Deref for PluginInstanceMap {
    type Target = HashMap<PluginInstanceId, Value>;

    fn deref(&self) -> &Self::Target {
        &self.plugins
    }
}

impl DerefMut for PluginInstanceMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.plugins
    }
}

impl Serialize for PluginInstanceMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let map = self.plugins.iter().map(|(k, v)| (k.to_string(), PluginConfig { id: k.clone(), spec: v.clone() })).collect::<HashMap<String, PluginConfig>>();
        map.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PluginInstanceMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map = HashMap::<String, PluginConfig>::deserialize(deserializer)?;
        let map = map
            .into_iter()
            .filter_map(|(k, v)| match PluginInstanceId::parse_by_code(v.id.code.clone(), &k) {
                Ok(id) => Some((id, v.spec)),
                Err(e) => {
                    eprintln!("failed to parse plugin instance id: {}", e);
                    None
                }
            })
            .collect();
        Ok(PluginInstanceMap { plugins: map })
    }
}

impl FromIterator<(PluginInstanceId, Value)> for PluginInstanceMap {
    fn from_iter<T: IntoIterator<Item = (PluginInstanceId, Value)>>(iter: T) -> Self {
        let map = iter.into_iter().collect();
        PluginInstanceMap { plugins: map }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub struct PluginMetaData {
    pub authors: Option<Cow<'static, str>>,
    pub description: Option<Cow<'static, str>>,
    pub version: Option<Cow<'static, str>>,
    pub homepage: Option<Cow<'static, str>>,
    pub repository: Option<Cow<'static, str>>,
}

#[macro_export]
macro_rules! plugin_meta {
    () => {
        {
            $crate::PluginMetaData {
                authors: Some(env!("CARGO_PKG_AUTHORS").into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                description: Some(env!("CARGO_PKG_DESCRIPTION").into()),
                homepage: Some(env!("CARGO_PKG_HOMEPAGE").into()),
                repository: Some(env!("CARGO_PKG_REPOSITORY").into()),
            }
        }
    };
    ($($key:ident: $value:expr),*) => {
        {
            let mut meta = $crate::plugin_meta!();
            $(
                meta.$key = Some($value.into());
            )*
            meta
        }
    };

}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub struct PluginAttributes {
    pub meta: PluginMetaData,
    pub mono: bool,
    pub code: Cow<'static, str>,
}
#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;
    #[test]
    fn test_inst_map_serde() {
        let mut map = PluginInstanceMap::default();
        let id_1 = PluginInstanceId {
            code: "header-modifier".into(),
            name: PluginInstanceName::anon(0),
        };
        map.insert(id_1.clone(), json!(null));
        let id_2 = PluginInstanceId {
            code: "header-modifier".into(),
            name: PluginInstanceName::anon(1),
        };
        map.insert(id_2.clone(), json!(null));

        let ser = serde_json::to_string(&map).unwrap();
        println!("{}", ser);
        let de: PluginInstanceMap = serde_json::from_str(&ser).unwrap();
        assert_eq!(map.get(&id_1), de.get(&id_1));
        assert_eq!(map.get(&id_2), de.get(&id_2));
    }

    #[test]
    fn test_parse_name() {
        assert_eq!("a-1".parse::<PluginInstanceName>().unwrap(), PluginInstanceName::anon(1));
        assert_eq!("n-my-plugin".parse::<PluginInstanceName>().unwrap(), PluginInstanceName::Named { name: "my-plugin".into() });
        assert_eq!("g".parse::<PluginInstanceName>().unwrap(), PluginInstanceName::Mono {});
        assert_ne!(
            "n-my-plugin".parse::<PluginInstanceName>().unwrap(),
            PluginInstanceName::Named { name: "my-plugin2".into() }
        );
        assert_ne!("g".parse::<PluginInstanceName>().unwrap(), PluginInstanceName::anon(1));
        assert!("".parse::<PluginInstanceName>().is_err());
        // assert!("a-".parse::<PluginInstanceName>().is_err());
        assert!("n-".parse::<PluginInstanceName>().is_err());
        // assert!("n-my-plugin-".parse::<PluginInstanceName>().is_err());
        assert!("g-".parse::<PluginInstanceName>().is_ok());
    }

    #[test]
    fn test_parse_id() {
        let json = r#"{"code": "header-modifier", "kind": "named", "name": "hello"}"#;
        let id: PluginInstanceId = serde_json::from_str(json).expect("fail to deserialize");
        println!("{id:?}");
    }
    #[test]
    fn test_dec() {
        let config = json!(
            {
                "code": "header-modifier",
                "kind": "anon",
                "uid": '0',
                "spec": null
            }
        );
        let cfg = PluginConfig::deserialize(config).unwrap();
        assert_eq!(cfg.id.code, "header-modifier");
        assert_eq!(cfg.id.name, PluginInstanceName::anon(0));

        let config = json!(
            {
                "code": "header-modifier",
                "spec": null,
                "kind": "mono",
            }
        );

        let cfg = PluginConfig::deserialize(config).unwrap();
        assert_eq!(cfg.id.code, "header-modifier");
        assert_eq!(cfg.id.name, PluginInstanceName::Mono {});

        let config = json!(
            {
                "code": "header-modifier",
                "name": "my-header-modifier",
                "kind": "named",
                "spec": null
            }
        );

        let cfg = PluginConfig::deserialize(config).unwrap();
        assert_eq!(cfg.id.code, "header-modifier");
    }
}
