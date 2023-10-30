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
    let query = fuzzy_.replace_all(query.as_ref(), |caps: &regex::Captures| format!("{}.*{}", &caps["frist"], &caps["last"]));
    Ok(Regex::new(&format!("^{}$", query))?.into())
}

pub fn find_add_delete(new: Vec<String>, old: Vec<String>) -> (Vec<String>, Vec<String>) {
    let add: Vec<String> = new.iter().filter(|item| !old.contains(item)).cloned().collect();

    let delete: Vec<String> = old.into_iter().filter(|item| !new.contains(item)).collect();

    (add, delete)
}

#[cfg(test)]
mod test {
    use crate::helper::fuzzy_regex;

    #[test]
    fn test_fuzzy_regex() {
        assert!(fuzzy_regex("*").unwrap().is_match("8435"));
        assert!(fuzzy_regex("*").unwrap().is_match("8435*erf"));

        assert!(fuzzy_regex("a*").unwrap().is_match("a435gt"));
        assert!(!fuzzy_regex("a*").unwrap().is_match("sdfa435gt"));

        assert!(fuzzy_regex("*a").unwrap().is_match("435ga"));
        assert!(!fuzzy_regex("*a").unwrap().is_match("sdfa435gt"));

        assert!(fuzzy_regex("a*b").unwrap().is_match("a435gtb"));
        assert!(fuzzy_regex("a*b").unwrap().is_match("a!@#$%^&*()_+b"));
        assert!(!fuzzy_regex("a*b").unwrap().is_match("a435gt"));
    }
}
