use std::{borrow::Cow, env};

use serde::Deserialize;

use crate::error::{Error, Result};

pub trait Check<T> {
    fn check(self) -> Result<T>;
}

#[derive(Deserialize, Debug)]
pub struct Namespace<'a> {
    pub target: Cow<'a, str>,
    pub toc: bool,
    pub text: Cow<'a, str>,
}

#[derive(Debug, Deserialize)]
pub struct SiteConfig<'a> {
    pub title: Option<Cow<'a, str>>,
    pub description: Option<Cow<'a, str>>,
    #[serde(default = "default_lang")]
    pub lang: Cow<'a, str>,
    #[serde(default = "default_base")]
    pub base: Cow<'a, str>,
}

impl<'a> Check<CheckedSiteConfig<'a>> for SiteConfig<'a> {
    fn check(self) -> Result<CheckedSiteConfig<'a>> {
        let SiteConfig {
            title,
            description,
            lang,
            base,
        } = self;

        let title = title
            .or_else(|| env::var("YUQUE_SSG_TITLE").map(Cow::from).ok())
            .ok_or(Error::MissingFields(stringify!(title).into()))?;

        Ok(CheckedSiteConfig {
            title,
            description,
            lang,
            base,
        })
    }
}

pub struct CheckedSiteConfig<'a> {
    pub title: Cow<'a, str>,
    pub description: Option<Cow<'a, str>>,
    pub lang: Cow<'a, str>,
    pub base: Cow<'a, str>,
}

#[derive(Debug, Deserialize)]
pub struct GeneratorConfig<'a> {
    pub host: Option<Cow<'a, str>>,
    pub token: Option<Cow<'a, str>>,
    #[serde(default)]
    pub namespaces: Vec<Namespace<'a>>,
}

pub struct CheckedGeneratorConfig<'a> {
    pub host: Cow<'a, str>,
    pub token: Cow<'a, str>,
    pub namespaces: Vec<Namespace<'a>>,
}

impl<'a> Check<CheckedGeneratorConfig<'a>> for GeneratorConfig<'a> {
    fn check(self) -> Result<CheckedGeneratorConfig<'a>> {
        let GeneratorConfig {
            host,
            token,
            namespaces,
        } = self;

        let host = host
            .or_else(|| env::var("YUQUE_SSG_HOST").map(Cow::from).ok())
            .ok_or(Error::MissingFields(stringify!(host).into()))?;
        let token = token
            .or_else(|| env::var("YUQUE_SSG_TOKEN").map(Cow::from).ok())
            .ok_or(Error::MissingFields(stringify!(token).into()))?;

        Ok(CheckedGeneratorConfig {
            host,
            token,
            namespaces,
        })
    }
}

#[derive(Deserialize, Debug)]
pub struct Config<'a> {
    pub site: SiteConfig<'a>,
    pub generator: GeneratorConfig<'a>,
}

impl<'a> Check<(CheckedSiteConfig<'a>, CheckedGeneratorConfig<'a>)> for Config<'a> {
    fn check(self) -> Result<(CheckedSiteConfig<'a>, CheckedGeneratorConfig<'a>)> {
        let Config { site, generator } = self;

        Ok((site.check()?, generator.check()?))
    }
}

fn default_base<'a>() -> Cow<'a, str> {
    "/".into()
}

fn default_lang<'a>() -> Cow<'a, str> {
    "zh-CN".into()
}
