use std::{
    borrow::Cow,
    collections::HashMap,
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
#[serde(untagged)]
pub enum PluginInstanceName {
    Anon {
        uid: u64,
    },
    Named {
        /// name should be unique within the plugin code, composed of alphanumeric characters and hyphens
        name: String,
    },
    Mono {},
}

impl PluginInstanceName {
    pub fn named(name: impl Into<String>) -> Self {
        PluginInstanceName::Named { name: name.into() }
    }
    pub fn mono() -> Self {
        PluginInstanceName::Mono {}
    }
    pub fn anon(uid: u64) -> Self {
        PluginInstanceName::Anon { uid }
    }
}

#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct PluginInstanceId {
    pub code: Cow<'static, str>,
    #[serde(flatten)]
    pub name: PluginInstanceName,
}

impl ToString for PluginInstanceId {
    fn to_string(&self) -> String {
        format!("{}-{}", self.code, self.name.to_string())
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
}

impl ToString for PluginInstanceName {
    fn to_string(&self) -> String {
        match &self {
            PluginInstanceName::Anon { uid } => format!("a-{:016x}", uid),
            PluginInstanceName::Named { name } => format!("n-{}", name),
            PluginInstanceName::Mono {} => "m".to_string(),
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
                Ok(PluginInstanceName::Anon {
                    uid: u64::from_str_radix(uid, 16)?,
                })
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

#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, rename = "PluginInstanceMap"))]
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

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;
    #[test]
    fn test_inst_map_serde() {
        let mut map = PluginInstanceMap::default();
        let id_1 = PluginInstanceId {
            code: "header-modifier".into(),
            name: PluginInstanceName::Anon { uid: 0 },
        };
        map.insert(id_1.clone(), json!(null));
        let id_2 = PluginInstanceId {
            code: "header-modifier".into(),
            name: PluginInstanceName::Anon { uid: 1 },
        };
        map.insert(id_2.clone(), json!(null));

        let ser = serde_json::to_string(&map).unwrap();
        println!("{}", ser);
        let de: PluginInstanceMap = serde_json::from_str(&ser).unwrap();
        assert_eq!(map.get(&id_1), de.get(&id_1));
        assert_eq!(map.get(&id_2), de.get(&id_2));
    }

    #[test]
    fn test_parse_id() {
        assert_eq!("a-0000000000000001".parse::<PluginInstanceName>().unwrap(), PluginInstanceName::Anon { uid: 1 });
        assert_eq!("n-my-plugin".parse::<PluginInstanceName>().unwrap(), PluginInstanceName::Named { name: "my-plugin".into() });
        assert_eq!("g".parse::<PluginInstanceName>().unwrap(), PluginInstanceName::Mono {});
        assert_ne!("a-0000000000000001".parse::<PluginInstanceName>().unwrap(), PluginInstanceName::Anon { uid: 2 });
        assert_ne!(
            "n-my-plugin".parse::<PluginInstanceName>().unwrap(),
            PluginInstanceName::Named { name: "my-plugin2".into() }
        );
        assert_ne!("g".parse::<PluginInstanceName>().unwrap(), PluginInstanceName::Anon { uid: 1 });
        assert!("".parse::<PluginInstanceName>().is_err());
        // assert!("a-".parse::<PluginInstanceName>().is_err());
        assert!("n-".parse::<PluginInstanceName>().is_err());
        assert!("a-0000000000000001-".parse::<PluginInstanceName>().is_err());
        // assert!("n-my-plugin-".parse::<PluginInstanceName>().is_err());
        assert!("g-".parse::<PluginInstanceName>().is_ok());
    }

    #[test]
    fn test_dec() {
        let config = json!(
            {
                "code": "header-modifier",
                "uid": 0,
                "spec": null
            }
        );
        let cfg = PluginConfig::deserialize(config).unwrap();
        assert_eq!(cfg.id.code, "header-modifier");
        assert_eq!(cfg.id.name, PluginInstanceName::Anon { uid: 0 });

        let config = json!(
            {
                "code": "header-modifier",
                "spec": null
            }
        );

        let cfg = PluginConfig::deserialize(config).unwrap();
        assert_eq!(cfg.id.code, "header-modifier");
        assert_eq!(cfg.id.name, PluginInstanceName::Mono {});

        let config = json!(
            {
                "code": "header-modifier",
                "name": "my-header-modifier",
                "spec": null
            }
        );

        let cfg = PluginConfig::deserialize(config).unwrap();
        assert_eq!(cfg.id.code, "header-modifier");
    }
}
