//! Git hosts (GitHub, GitHub Enterprise, GitLab, Bitbucket, any https git
//! host): where a repository's archive lives, and the credential for exactly
//! that host: a token variable, else what `git credential fill` returns.

mod auth;
mod host;

use super::Origin;

pub use auth::{credential_fill, credentials, git_header};
pub use host::{kind_of, parse_repo_url};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    GitHub,
    GhEnterprise,
    GitLab,
    Bitbucket,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub origin: Origin,
    pub kind: Kind,
}

#[cfg(test)]
mod tests;
