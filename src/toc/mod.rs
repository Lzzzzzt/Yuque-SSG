use std::{
    borrow::Cow,
    fs::File,
    io::{BufRead, BufReader, Write},
    path::Path,
};

use derive_builder::Builder;
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

pub mod generate;
pub mod parse;

#[derive(Serialize, Builder, Clone, Debug)]
pub struct SidebarItem {
    text: String,
    link: String,
    #[builder(default = "None")]
    items: Option<Vec<SidebarItem>>,
    #[builder(default = "None")]
    collapsed: Option<bool>,
    #[serde(skip)]
    #[builder(default)]
    order: u32,
}

impl SidebarItem {
    pub fn builder() -> SidebarItemBuilder {
        SidebarItemBuilder::default()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Builder)]
pub struct Frontmatter<'a> {
    #[builder(default)]
    title: Option<Cow<'a, str>>,
    #[builder(default)]
    #[serde(rename = "titleTemplate")]
    title_template: Option<Cow<'a, str>>,
    sidebar: Cow<'a, str>,
    order: u32,
    #[builder(default)]
    description: Option<Cow<'a, str>>,
    #[builder(default = "true")]
    have_content: bool,
}

impl<'a> Frontmatter<'a> {
    pub fn builder() -> FrontmatterBuilder<'a> {
        FrontmatterBuilder::default()
    }

    pub fn write_to(&self, w: &mut impl Write) {
        w.write_all(b"---\n").ok();
        serde_yaml::to_writer(w.by_ref(), self).ok();
        w.write_all(b"---\n").ok();
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        debug!("Parsing frontmatter of: {}", path.as_ref().display());

        let reader = BufReader::new(File::open(path)?);

        let mut frontmatter = String::new();

        let mut in_frontmatter = 0;

        for line in reader.lines() {
            let line = line.expect("Invalid String");

            if line.starts_with("---") {
                in_frontmatter += 1;
                continue;
            }

            if in_frontmatter == 1 {
                frontmatter.push_str(&line);
                frontmatter.push('\n');
            }

            if in_frontmatter > 1 {
                break;
            }
        }

        debug!("frontmatter: \n{}", frontmatter);

        serde_yaml::from_str::<Frontmatter>(&frontmatter)
            .map_err(|e| Error::Internal(e.to_string()))
    }
}

#[derive(Deserialize, Debug, Serialize)]
pub struct NavbarItem {
    pub text: String,
    pub link: String,
    pub items: Option<Vec<NavbarItem>>,
}

impl From<(&str, &str)> for NavbarItem {
    fn from(value: (&str, &str)) -> Self {
        Self {
            text: value.0.into(),
            link: value.1.into(),
            items: None,
        }
    }
}

impl From<NavbarItem> for Value {
    fn from(value: NavbarItem) -> Self {
        serde_json::json!({
            "text": value.text,
            "link": value.link,
            "items": value.items
        })
    }
}
