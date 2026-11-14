//! 文件名匹配：子串或 glob，都可选区分大小写。

use crate::params::MatchMode;

pub enum Matcher {
    Substring {
        needle: String,
        case_sensitive: bool,
    },
    Glob(globset::GlobMatcher),
}

impl Matcher {
    pub fn new(query: &str, mode: MatchMode, case_sensitive: bool) -> Result<Self, String> {
        match mode {
            MatchMode::Substring => Ok(Self::Substring {
                needle: if case_sensitive {
                    query.to_owned()
                } else {
                    query.to_lowercase()
                },
                case_sensitive,
            }),
            MatchMode::Glob => globset::GlobBuilder::new(query)
                .case_insensitive(!case_sensitive)
                .literal_separator(true)
                .build()
                .map(|glob| Self::Glob(glob.compile_matcher()))
                .map_err(|err| format!("glob {query} 写错了：{err}")),
        }
    }

    pub fn matches(&self, name: &str) -> bool {
        match self {
            Self::Substring {
                needle,
                case_sensitive,
            } => {
                if *case_sensitive {
                    name.contains(needle.as_str())
                } else {
                    name.to_lowercase().contains(needle.as_str())
                }
            }
            Self::Glob(glob) => glob.is_match(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substring_can_ignore_case() {
        let matcher = Matcher::new("readme", MatchMode::Substring, false).unwrap();
        assert!(matcher.matches("README.md"));
        let matcher = Matcher::new("readme", MatchMode::Substring, true).unwrap();
        assert!(!matcher.matches("README.md"));
    }

    #[test]
    fn glob_stays_in_one_segment() {
        let matcher = Matcher::new("*.txt", MatchMode::Glob, false).unwrap();
        assert!(matcher.matches("a.txt"));
        assert!(!matcher.matches("a/b.txt"));
    }
}
