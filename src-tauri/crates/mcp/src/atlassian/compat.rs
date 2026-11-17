//! Reading the setup from YAML/JSON, including older files that stored credentials on each product.

use serde::{Deserialize, Deserializer};

use super::{has_credential, Atlassian, Service};

/// Reads a YAML/JSON scalar into a string: null is empty, numbers and booleans keep their text.
pub(super) fn lenient_string<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    struct V;
    impl<'de> serde::de::Visitor<'de> for V {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a scalar")
        }
        fn visit_str<E>(self, v: &str) -> std::result::Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_string<E>(self, v: String) -> std::result::Result<String, E> {
            Ok(v)
        }
        fn visit_i64<E>(self, v: i64) -> std::result::Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_u64<E>(self, v: u64) -> std::result::Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_f64<E>(self, v: f64) -> std::result::Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_bool<E>(self, v: bool) -> std::result::Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_unit<E>(self) -> std::result::Result<String, E> {
            Ok(String::new())
        }
        fn visit_none<E>(self) -> std::result::Result<String, E> {
            Ok(String::new())
        }
    }
    d.deserialize_any(V)
}

#[derive(Deserialize)]
struct AtlassianRaw {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default, deserialize_with = "lenient_string")]
    username: String,
    #[serde(default, deserialize_with = "lenient_string")]
    api_token: String,
    #[serde(default)]
    jira: Option<ServiceRaw>,
    #[serde(default)]
    confluence: Option<ServiceRaw>,
    #[serde(default, rename = "confluence_same_as_jira")]
    same: Option<bool>,
}

impl<'de> Deserialize<'de> for Atlassian {
    /// Reads the current setup and older files that stored credentials on each product.
    /// A null value leaves the zero setup.
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        match Option::<AtlassianRaw>::deserialize(d)? {
            None => Ok(Atlassian::default()),
            Some(raw) => Ok(raw.into_atlassian()),
        }
    }
}

impl AtlassianRaw {
    fn into_atlassian(self) -> Atlassian {
        if let Some(enabled) = self.enabled {
            return Atlassian {
                enabled,
                username: self.username,
                api_token: self.api_token,
                jira: ServiceRaw::service(self.jira.as_ref()),
                confluence: ServiceRaw::service(self.confluence.as_ref()),
            }
            .normalized();
        }
        let jira = legacy(self.jira.as_ref());
        let wiki = legacy(self.confluence.as_ref());
        let same = self.same.unwrap_or(false);
        let (mut shared_user, mut shared_api) = (jira.username.clone(), jira.api_token.clone());
        let mut wiki_own = !same && wiki.has_credential() && !same_credential(&jira, &wiki);
        if !has_credential(&shared_user, &shared_api) {
            shared_user = wiki.username.clone();
            shared_api = wiki.api_token.clone();
            wiki_own = false;
        }
        let mut confluence = Service {
            enabled: wiki.present(),
            url: wiki.url.clone(),
            ..Default::default()
        };
        if wiki_own {
            confluence.own = true;
            confluence.username = wiki.username.clone();
            confluence.api_token = wiki.api_token.clone();
        }
        Atlassian {
            enabled: jira.present() || wiki.present(),
            username: shared_user,
            api_token: shared_api,
            jira: Service {
                enabled: jira.present(),
                url: jira.url.clone(),
                ..Default::default()
            },
            confluence,
        }
        .normalized()
    }
}

#[derive(Deserialize)]
struct ServiceRaw {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default, deserialize_with = "lenient_string")]
    url: String,
    #[serde(default, rename = "own_credentials")]
    own: Option<bool>,
    #[serde(default, deserialize_with = "lenient_string")]
    username: String,
    #[serde(default, deserialize_with = "lenient_string")]
    api_token: String,
    #[serde(default, deserialize_with = "lenient_string")]
    board_id: String,
    #[serde(default, deserialize_with = "lenient_string")]
    board_name: String,
    #[serde(default, deserialize_with = "lenient_string")]
    space_id: String,
    #[serde(default, deserialize_with = "lenient_string")]
    space_key: String,
    #[serde(default, deserialize_with = "lenient_string")]
    space_name: String,
    #[serde(default, deserialize_with = "lenient_string")]
    parent_id: String,
    #[serde(default, deserialize_with = "lenient_string")]
    parent_title: String,
}

impl ServiceRaw {
    fn service(raw: Option<&ServiceRaw>) -> Service {
        let Some(s) = raw else {
            return Service::default();
        };
        Service {
            enabled: s.enabled.unwrap_or(false),
            url: s.url.clone(),
            own: s.own.unwrap_or(false),
            username: s.username.clone(),
            api_token: s.api_token.clone(),
            board_id: s.board_id.clone(),
            board_name: s.board_name.clone(),
            space_id: s.space_id.clone(),
            space_key: s.space_key.clone(),
            space_name: s.space_name.clone(),
            parent_id: s.parent_id.clone(),
            parent_title: s.parent_title.clone(),
        }
    }
}

fn legacy(raw: Option<&ServiceRaw>) -> Service {
    let Some(s) = raw else {
        return Service::default();
    };
    Service {
        url: s.url.clone(),
        username: s.username.clone(),
        api_token: s.api_token.clone(),
        ..Default::default()
    }
}

fn same_credential(a: &Service, b: &Service) -> bool {
    a.username == b.username && a.api_token == b.api_token
}
