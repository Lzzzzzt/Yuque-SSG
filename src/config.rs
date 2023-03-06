use std::borrow::Cow;

use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Deserialize, Debug)]
pub struct Namespace<'a> {
    pub target: Cow<'a, str>,
    pub toc: bool,
    pub text: Cow<'a, str>,
}

#[derive(Deserialize, Debug)]
pub struct Config<'a> {
    pub title: Option<Cow<'a, str>>,
    pub description: Option<Cow<'a, str>>,
    #[serde(default = "default_lang")]
    pub lang: Cow<'a, str>,
    #[serde(default = "default_base")]
    pub base: Cow<'a, str>,
    pub host: Option<Cow<'a, str>>,
    pub token: Option<Cow<'a, str>>,
    #[serde(default)]
    pub namespaces: Vec<Namespace<'a>>,
}

impl<'a> Config<'a> {
    pub fn check(self) -> Result<CheckedConfig<'a>> {
        let Config {
            title,
            description,
            lang,
            base,
            host,
            token,
            namespaces,
        } = self;

        let title = title.ok_or(Error::MissingFields(stringify!(title).into()))?;
        let host = host.ok_or(Error::MissingFields(stringify!(host).into()))?;
        let token = token.ok_or(Error::MissingFields(stringify!(token).into()))?;

        Ok(CheckedConfig {
            title,
            description,
            lang,
            base,
            host,
            token,
            namespaces,
        })
    }
}

pub struct CheckedConfig<'a> {
    pub title: Cow<'a, str>,
    pub description: Option<Cow<'a, str>>,
    pub lang: Cow<'a, str>,
    pub base: Cow<'a, str>,
    pub host: Cow<'a, str>,
    pub token: Cow<'a, str>,
    pub namespaces: Vec<Namespace<'a>>,
}

fn default_base<'a>() -> Cow<'a, str> {
    "/".into()
}

fn default_lang<'a>() -> Cow<'a, str> {
    "zh-CN".into()
}

pub struct GeneratorConfig<'a> {
    pub host: Cow<'a, str>,
    pub token: Cow<'a, str>,
    pub base: Cow<'a, str>,
    pub namespaces: Vec<Namespace<'a>>,
}
