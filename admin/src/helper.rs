#[cfg(feature = "k8s")]
use kube::Client;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::regex;
use tardis::regex::Regex;

#[cfg(feature = "k8s")]
pub async fn get_k8s_client() -> TardisResult<Client> {
    Client::try_default().await.map_err(|error| TardisError::wrap(&format!("[SG.admin] Get kubernetes client error: {error:?}"), ""))
}

/// Convert fuzzy search queries into regular instance
pub fn fuzzy_regex(query: impl AsRef<str>) -> TardisResult<Regex> {
    let fuzzy_ = Regex::new(r#"(?<frist>[^\\]?)\*(?<last>\w*)"#)?;
    println!("====?:{}", fuzzy_.is_match(query.as_ref()));
    let query = fuzzy_.replace_all(query.as_ref(), |caps: &regex::Captures| {
        println!("{:?}", caps);
        format!("{}.*{}", &caps["frist"], &caps["last"])
    });
    println!("{}", query);
    Ok(Regex::new(&format!("^{}$", query))?.into())
}
#[cfg(test)]
mod test {
    use crate::helper::fuzzy_regex;

    #[test]
    fn test_fuzzy_regex() {
        assert!(fuzzy_regex("*").unwrap().is_match("8435"));
        assert!(!fuzzy_regex(r#"\*"#).unwrap().is_match(r"\*"));

        assert!(fuzzy_regex("*").unwrap().is_match("8435*erf"));

        assert!(fuzzy_regex("a*").unwrap().is_match("a435gt"));
        assert!(!fuzzy_regex("a*").unwrap().is_match("sdfa435gt"));

        assert!(fuzzy_regex("a*b").unwrap().is_match("a435gtb"));
        assert!(fuzzy_regex("a*b").unwrap().is_match("a!@#$%^&*()_+b"));
        assert!(!fuzzy_regex("a*b").unwrap().is_match("a435gt"));
    }
}
