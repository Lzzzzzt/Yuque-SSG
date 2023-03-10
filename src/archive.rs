#![allow(unused)]

use std::io::{Cursor, Write};

use base64::Engine;
use comrak::{
    format_commonmark,
    nodes::{AstNode, NodeHtmlBlock, NodeValue},
    parse_document, Arena, ComrakOptions,
};
use image::{DynamicImage, ImageOutputFormat};
use log::{info, warn};
use tokio::io::AsyncWriteExt;

use crate::USER_AGENT;
use crate::{config::CheckedSiteConfig, error::Result};

pub type FormatFunction<'a> = fn(&'a AstNode<'a>) -> Result<()>;

pub struct Formatter<'a> {
    root: Option<&'a AstNode<'a>>,
    arena: Arena<AstNode<'a>>,
    options: ComrakOptions,
}

impl<'a> Default for Formatter<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Formatter<'a> {
    pub fn new() -> Self {
        let arena = Arena::new();
        let mut options = ComrakOptions::default();

        options.extension.superscript = true;
        options.extension.table = true;

        options.parse.default_info_string = Some("text".into());

        Self {
            root: None,
            arena,
            options,
        }
    }

    pub fn parse(&'a mut self, markdown: &str) -> &Self {
        let root = parse_document(&self.arena, markdown, &self.options);
        self.root = Some(root);
        self
    }

    pub fn format(&self, func: FormatFunction<'a>) -> &Self {
        if let Some(root) = self.root {
            Self::iter_nodes(root, func);
            return self;
        }
        warn!("Can not format before parse.");
        self
    }

    fn iter_nodes<'n>(node: &'n AstNode<'n>, f: FormatFunction<'n>) {
        f(node).ok();
        for c in node.children() {
            Self::iter_nodes(c, f);
        }
    }

    pub fn write_to(&self, file: &mut impl Write) {
        let mut file = file;
        if let Some(root) = self.root {
            format_commonmark(root, &self.options, &mut file).ok();
            return;
        }

        warn!("Can not format before parse.");
    }
}

fn convert_codepen_link_to_iframe<'a>(node: &'a AstNode<'a>) -> Result<()> {
    let mut url = String::new();
    let mut content = String::new();

    if let &NodeValue::Link(ref link) = &node.data.borrow().value {
        url = String::from_utf8_lossy(&link.url).to_string();
        if let &NodeValue::Text(ref text) = &node.first_child().unwrap().data.borrow().value {
            content = String::from_utf8_lossy(text).to_string();
        }
    }

    if !url.is_empty() {
        url::Url::parse(&url).ok().into_iter().for_each(|origin_url| {
                        if let Some(domain) = origin_url.domain() {
                            if domain.contains("codepen") {
                                let literal = format!(r#"
<iframe height="300" style="width: 100%;" scrolling="no" title="Untitled" src="{}" frameborder="no" loading="lazy" allowtransparency="true" allowfullscreen="true">{}</iframe>
                                "#, url, content).as_bytes().to_vec();

                                node.children().for_each(|node| node.detach());

                                let mut data = node.data.borrow_mut();
                                data.value = NodeValue::HtmlBlock(NodeHtmlBlock {
                                        block_type: 6,
                                        literal,
                                    })
                            }
                        }
                    })
    }

    Ok(())
}

fn image_to_base64(img: &DynamicImage) -> String {
    let mut image_data: Vec<u8> = Vec::new();
    img.write_to(&mut Cursor::new(&mut image_data), ImageOutputFormat::Png)
        .unwrap();

    info!("Convert image to base64 string.");

    let res_base64 = base64::prelude::BASE64_STANDARD.encode(image_data);
    format!("data:image/png;base64,{}", res_base64)
}

fn convert_image_to_base64<'a>(node: &'a AstNode<'a>) -> Result<()> {
    let mut svg = vec![];

    if let NodeValue::Image(i) = &mut node.data.borrow_mut().value {
        let url = String::from_utf8_lossy(&i.url).to_string();
        let url = url::Url::parse(&url)?;

        info!("Find image url: {}", url);

        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .build()?;

        let response = client.get(url).send()?;

        let bytes = response.bytes()?;

        if bytes.starts_with(b"<svg") {
            svg = bytes.into();
        } else {
            let image = image::load_from_memory(&bytes)?;
            let bs = image_to_base64(&image);
            i.url = bs.as_bytes().to_vec();
        }
    }

    if !svg.is_empty() {
        node.data.borrow_mut().value = NodeValue::HtmlBlock(NodeHtmlBlock {
            block_type: 7,
            literal: svg,
        })
    }

    Ok(())
}

impl<'a> CheckedSiteConfig<'a> {
    pub async fn gen_js(&self) -> Result<()> {
        info!("Generating `config.js`");

        let file_path = std::path::Path::new("./docs/.vitepress/config.js");

        if file_path.exists() {
            info!("Find existed `config.js`, rename it to `config.old.js`");
            let new_file_path = file_path.parent().unwrap().join("config.old.js");
            tokio::fs::rename(file_path, new_file_path).await?;
        }

        if !file_path.parent().unwrap().exists() {
            std::fs::create_dir_all(file_path.parent().unwrap())?;
        }

        let mut js_file = tokio::fs::File::create(file_path).await?;

        let CheckedSiteConfig {
            title,
            lang,
            description,
            base,
            host: _,
            port: _,
            theme: _,
        } = &self;

        let content = format!(
            r#"
    import {{ defineConfig }} from 'vitepress'
    import nav from '../../nav.json'
    import sidebar from '../../sidebar.json'
    
    import taskListPlugin from "markdown-it-task-lists"
    import subPlugin from "markdown-it-sub"
    
    export default defineConfig({{
    themeConfig: {{
        "nav": [...nav,],
        "sidebar": {{ ...sidebar }},
    }},
    title: "{}",
    lang: "{}",
    base: "{}",
    description: "{}",
    appearance: true,
    }})
            "#,
            title,
            lang,
            base,
            description.clone().unwrap_or_default(),
        );

        js_file.write_all(content.as_bytes()).await?;

        Ok(())
    }
}
