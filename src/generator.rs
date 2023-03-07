use std::time::Duration;
use std::{io::Write, iter::zip, ops::Not, path::PathBuf};

use comrak::{
    format_commonmark,
    nodes::{AstNode, NodeHtmlBlock, NodeValue},
    parse_document, Arena, ComrakOptions,
};
use log::{debug, info, warn};
use tokio::time::sleep;
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};
use yuque_rust::{DocsClient, Toc, Yuque};

use crate::toc::parse::Pinyin;
use crate::{
    config::{CheckedGeneratorConfig, Namespace},
    error::Result,
    toc::{generate::generate_doc_sidebar, parse::parse_toc_structure, Frontmatter, NavbarItem},
};

#[allow(unused)]
pub struct Generator<'n> {
    client: Yuque,
    namespaces: Vec<Namespace<'n>>,
}

impl<'n> Generator<'n> {
    pub fn from_config(config: CheckedGeneratorConfig<'n>) -> Self {
        let CheckedGeneratorConfig {
            host,
            token,
            namespaces,
        } = config;

        let client = Yuque::builder()
            .host(host.into())
            .token(token.into())
            .build()
            .unwrap();

        Self { client, namespaces }
    }

    pub async fn generate(&self) -> Result<()> {
        let mut navbar = vec![];

        for namespace in self.namespaces.iter() {
            let name = &namespace.target;
            let text = &namespace.text;
            let toc = namespace.toc;

            let repos = self.client.repos();
            let docs = self.client.docs();

            if toc {
                let response = match repos.get(name, None).await {
                    Ok(response) => response.data,
                    Err(e) => {
                        warn!("Can not get the repo info due to {}.", e);
                        warn!("retry after 3s.");
                        sleep(Duration::from_secs(3)).await;
                        repos.get(name, None).await?.data
                    }
                };

                let description = response.description.unwrap_or_default();
                let mut toc = response.toc.unwrap();

                toc.remove(0);

                let ns_path = &response.name.to_lowercase();

                let paths = parse_toc_structure(ns_path, &toc);

                for (i, (path, item)) in zip(paths, toc).enumerate() {
                    match Self::write_markdown_with_toc(&docs, path, name, item, i).await {
                        Ok(_) => (),
                        Err(e) => {
                            warn!("Can not write the file due to {}.", e);
                            warn!("Skip.");
                        }
                    }
                }

                navbar.push(NavbarItem {
                    text: text.to_string(),
                    link: format!("/{}/", ns_path),
                });

                let mut index_file = File::create(format!("docs/{}/index.md", &ns_path)).await?;

                index_file
                    .write_all(format!("# {}\n", response.name).as_bytes())
                    .await?;

                index_file.write_all(description.as_bytes()).await?;
            } else {
                let response = match repos.get(name, None).await {
                    Ok(response) => response.data,
                    Err(e) => {
                        warn!("Can not get the repo info due to {}.", e);
                        warn!("retry after 3s.");
                        sleep(Duration::from_secs(3)).await;
                        repos.get(name, None).await?.data
                    }
                };
                let ns_name = response.name.to_string();
                let ns_path = ns_name.to_lowercase();
                let description = response.description.unwrap_or_default();
                let response = docs.list_with_repo(name).await?.data;

                for (i, item) in response.into_iter().enumerate() {
                    let path = PathBuf::from(format!(
                        "docs/{}/{}.md",
                        ns_path,
                        item.title.to_pinyin_or_lowercase()
                    ));

                    match Self::write_markdown(&docs, path, name, item.id as u32, i).await {
                        Ok(_) => (),
                        Err(e) => {
                            warn!("Can not write the file due to {}.", e);
                            warn!("Skip.");
                        }
                    }
                }

                navbar.push(NavbarItem {
                    text: text.to_string(),
                    link: format!("/{}/", ns_path),
                });

                let mut index_file = File::create(format!("docs/{}/index.md", &ns_path)).await?;

                index_file
                    .write_all(format!("# {}\n", ns_name).as_bytes())
                    .await?;

                index_file.write_all(description.as_bytes()).await?;
            }
        }

        generate_doc_sidebar("./docs")?;

        File::create("./nav.json")
            .await?
            .write_all(
                serde_json::json!({
                    "text": "知识库",
                    "items": navbar,
                })
                .to_string()
                .as_bytes(),
            )
            .await?;

        info!("Generate navbar config.");

        Ok(())
    }

    async fn write_markdown_with_toc(
        client: &DocsClient,
        path: PathBuf,
        ns: &str,
        doc: Toc<'_>,
        order: usize,
    ) -> Result<()> {
        match doc {
            Toc::Doc(doc) => {
                Self::write_markdown(client, path, ns, doc.id, order).await?;
            }
            Toc::Title(title) => {
                if path.exists().not() {
                    fs::create_dir_all(&path).await?;
                }

                let file_path = path.join("index.md");

                let mut file = std::fs::File::create(file_path)?;

                Frontmatter::builder()
                    .sidebar(title.title.clone())
                    .order(order as u32)
                    .have_content(false)
                    .title_template(Some(title.title.clone()))
                    .build()?
                    .write_to(&mut file);

                debug!("Write frontmatter to: index.md",);
            }
            _ => (),
        }

        Ok(())
    }

    async fn write_markdown(
        client: &DocsClient,
        path: PathBuf,
        ns: &str,
        id: u32,
        order: usize,
    ) -> Result<()> {
        let doc = client
            .get_with_repo_ns(ns, id, Some(&[("raw", "1")]))
            .await?
            .data;

        info!("Find doc: {}", doc.title);

        debug!("doc path: {}", path.display());

        let parent_path = path.parent().unwrap();

        if parent_path.exists().not() {
            fs::create_dir_all(parent_path).await?;
        }

        let mut file = std::fs::File::create(&path)?;

        Frontmatter::builder()
            .sidebar(doc.title.clone())
            .order(order as u32)
            .title_template(Some(doc.title.clone()))
            .build()?
            .write_to(&mut file);

        debug!(
            "Write frontmatter to: {}",
            path.file_name().unwrap().to_string_lossy()
        );

        file.write_all(format!("# {}\n", doc.title).as_bytes())?;

        Formatter::new()
            .parse(&doc.body)
            .format(convert_codepen_link_to_iframe)
            .write_to(&mut file);

        debug!("Write File to: {}", path.display());

        Ok(())
    }
}

impl<'a> From<CheckedGeneratorConfig<'a>> for Generator<'a> {
    fn from(value: CheckedGeneratorConfig<'a>) -> Self {
        Self::from_config(value)
    }
}

pub type FormatFunction<'a> = fn(&'a AstNode<'a>);

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
        f(node);
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

fn convert_codepen_link_to_iframe<'a>(node: &'a AstNode<'a>) {
    let mut url = String::new();
    let mut content = String::new();

    if let &NodeValue::Link(ref link) = &node.data.borrow().value {
        url = String::from_utf8_lossy(&link.url).to_string();
        if let &NodeValue::Text(ref text) = &node.first_child().unwrap().data.borrow().value {
            content = String::from_utf8_lossy(text).to_string();
        }
    }

    if url.is_empty().not() {
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
}
