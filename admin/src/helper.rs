#[cfg(feature = "k8s")]
use kube::Client;

use tardis::basic::result::TardisResult;
use tardis::regex;
use tardis::regex::Regex;

/// Convert fuzzy search queries into regular instance
pub fn fuzzy_regex(query: impl AsRef<str>) -> TardisResult<Regex> {
    let fuzzy_ = Regex::new(r"(?<frist>[^\\]?)\*(?<last>\w*)")?;
    let query = fuzzy_.replace_all(query.as_ref(), |caps: &regex::Captures| format!("{}.*{}", &caps["frist"], &caps["last"]));
    Ok(Regex::new(&format!("^{}$", query))?)
}

pub fn find_add_delete<T>(new: Vec<T>, old: Vec<T>) -> (Vec<T>, Vec<T>)
where
    T: std::cmp::PartialEq + std::clone::Clone,
{
    if new.is_empty() && old.is_empty() {
        return (vec![], vec![]);
    }
    if new.is_empty() {
        return (vec![], old);
    }
    if old.is_empty() {
        return (new, vec![]);
    }

    let add: Vec<T> = new.iter().filter(|item| !old.contains(item)).cloned().collect();

    let delete: Vec<T> = old.into_iter().filter(|item| !new.contains(item)).collect();

    (add, delete)
}

#[cfg(test)]
mod test {
    use crate::helper::fuzzy_regex;

    #[test]
    fn test_fuzzy_regex() {
        assert!(fuzzy_regex("*").unwrap().is_match("8435"));
        assert!(fuzzy_regex("*").unwrap().is_match("8435*erf"));
        assert!(!fuzzy_regex("").unwrap().is_match("dsfasd"));

        assert!(!fuzzy_regex("a").unwrap().is_match("sdfa435gt"));
        assert!(fuzzy_regex("a*").unwrap().is_match("a435gt"));
        assert!(!fuzzy_regex("a*").unwrap().is_match("sdfa435gt"));

        assert!(fuzzy_regex("*a").unwrap().is_match("435ga"));
        assert!(!fuzzy_regex("*a").unwrap().is_match("sdfa435gt"));

        assert!(fuzzy_regex("a*b").unwrap().is_match("a435gtb"));
        assert!(fuzzy_regex("a*b").unwrap().is_match("a!@#$%^&*()_+b"));
        assert!(!fuzzy_regex("a*b").unwrap().is_match("a435gt"));
    }
}
