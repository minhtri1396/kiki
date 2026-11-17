use anyhow::{anyhow, Result};

use crate::atlassian::{has_credential, parse_site, Atlassian, Service};

/// Where and as whom to call one Atlassian service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Login {
    /// `scheme://host[:port]` of the site, with no path.
    pub origin: String,
    pub user: String,
    pub token: String,
}

impl Login {
    /// Resolves the site and credentials the saved setup gives one service.
    /// `name` is the service's display name used in error messages.
    pub fn for_service(cfg: &Atlassian, service: &Service, name: &str) -> Result<Login> {
        let cfg = cfg.normalized();
        let service = service.clean();
        if !cfg.enabled || !service.enabled {
            return Err(anyhow!("turn on {name} in Settings"));
        }
        let (mut user, mut token) = (cfg.username.clone(), cfg.api_token.clone());
        if service.own {
            user = service.username.clone();
            token = service.api_token.clone();
        }
        if !has_credential(&user, &token) {
            return Err(anyhow!("set the Atlassian email and API token in Settings"));
        }
        if service.url.is_empty() {
            return Err(anyhow!("{name} site is required"));
        }
        match parse_site(&service.url) {
            Some(site) if site.scheme == "https" || site.scheme == "http" => Ok(Login {
                origin: format!("{}://{}", site.scheme, site.host),
                user,
                token,
            }),
            _ => Err(anyhow!("{name} site must be an http or https URL")),
        }
    }
}
