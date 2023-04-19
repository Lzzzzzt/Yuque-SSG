use std::{
    borrow::Cow,
    cell::RefCell,
    collections::HashMap,
    io::{Cursor, Write},
    iter::zip,
    ops::Not,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use base64::Engine;
use comrak::nodes::{AstNode, NodeHeading, NodeHtmlBlock, NodeLink, NodeValue};
use image::{DynamicImage, ImageOutputFormat};
use log::{debug, error, info, warn};
use regex::Regex;
use serde_json::Value;
use tokio::{
    fs::{self, remove_dir_all, File},
    io::AsyncWriteExt,
    sync::RwLock,
    time::sleep,
};
use yuque_rust::{DocsClient, Toc, Yuque};

use crate::{
    config::{CheckedGeneratorConfig, Namespace},
    error::{Error, Result},
    formatter::Formatter,
    run_display_command_output,
    toc::{
        generate::generate_doc_sidebar,
        parse::{parse_toc_structure, Pinyin},
        Frontmatter, NavbarItem,
    },
    CODEPEN_IFRAME, USER_AGENT,
};

pub struct Generator<'n> {
    inner: Arc<RwLock<GeneratorInner<'n>>>,
    pub article_path: RwLock<HashMap<String, HashMap<String, PathBuf>>>,
    pub schemas: Mutex<HashMap<String, Value>>,
}

pub struct GeneratorInner<'n> {
    pub client: Yuque,
    pub namespaces: Vec<Namespace<'n>>,
    pub ns_id_path: HashMap<i32, PathBuf>,
    pub id_ns: HashMap<i32, Namespace<'n>>,
    pub build_command: Cow<'n, str>,
}

impl<'n> Generator<'n> {
    pub fn from_config(config: CheckedGeneratorConfig<'n>) -> Self {
        let CheckedGeneratorConfig {
            host,
            token,
            namespaces,
            build_command,
        } = config;

        let client = Yuque::builder()
            .host(host.into())
            .token(token.into())
            .build()
            .unwrap();

        let article_path: HashMap<String, HashMap<String, PathBuf>> = HashMap::new();

        Self {
            inner: Arc::new(RwLock::new(GeneratorInner {
                client,
                ns_id_path: HashMap::with_capacity(namespaces.len()),
                id_ns: HashMap::with_capacity(namespaces.len()),
                namespaces,
                build_command,
            })),
            article_path: RwLock::new(article_path),
            schemas: Mutex::new(HashMap::new()),
        }
    }

    pub async fn generate_one(&self, ns: &Namespace<'n>) -> Result<(NavbarItem, (i32, PathBuf))> {
        let name = &ns.target;
        let text = &ns.text;
        let toc = ns.toc;

        let repos = self.inner.read().await.client.repos();
        let docs = self.inner.read().await.client.docs();

        let navbar_item: NavbarItem;
        let p: (i32, PathBuf);

        let mut article_path = self.article_path.write().await;

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

            article_path.insert(response.namespace.to_string(), HashMap::default());
            let ns_inner_path = article_path.get_mut(response.namespace.as_ref()).unwrap();

            toc.remove(0);

            let ns_path = &response.name.to_lowercase();

            let paths = parse_toc_structure(ns_path, &toc);

            for (path, item) in zip(&paths, &toc) {
                if let Toc::Doc(doc) = &item {
                    ns_inner_path.insert(doc.url.to_string(), path.clone());
                }
            }

            drop(article_path);

            for (i, (path, item)) in zip(paths, toc).enumerate() {
                match &self
                    .write_markdown_with_toc(&docs, path, name, item, i)
                    .await
                {
                    Ok(_) => (),
                    Err(e) => {
                        warn!("Can not write the file due to {}.", e);
                        warn!("Skip.");
                    }
                }
            }

            navbar_item = NavbarItem {
                text: text.to_string(),
                link: format!("/{}/", ns_path),
                items: None,
            };

            let mut index_file = File::create(format!("docs/{}/index.md", &ns_path)).await?;

            index_file
                .write_all(format!("# {}\n", response.name).as_bytes())
                .await?;

            index_file.write_all(description.as_bytes()).await?;

            p = (response.id, PathBuf::from(format!("docs/{}/", ns_path)));
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

            article_path.insert(response.namespace.to_string(), HashMap::default());
            let ns_inner_path = article_path.get_mut(response.namespace.as_ref()).unwrap();

            let book_id = response.id;
            let ns_name = response.name.to_string();
            let ns_path = ns_name.to_lowercase();
            let description = response.description.unwrap_or_default();
            let response = docs.list_with_repo(name).await?.data;

            for item in response.iter() {
                ns_inner_path.insert(
                    item.slug.to_string(),
                    PathBuf::from(format!(
                        "./docs/{}/{}.md",
                        ns_path,
                        item.title.to_pinyin_or_lowercase()
                    )),
                );
            }

            for (i, item) in response.into_iter().enumerate() {
                let path = PathBuf::from(format!(
                    "docs/{}/{}.md",
                    ns_path,
                    item.title.to_pinyin_or_lowercase()
                ));

                match self
                    .write_markdown(&docs, path, name, item.id as u32, i)
                    .await
                {
                    Ok(_) => (),
                    Err(e) => {
                        warn!("Can not write the file due to {}.", e);
                        warn!("Skip.");
                    }
                }
            }

            navbar_item = NavbarItem {
                text: text.to_string(),
                link: format!("/{}/", ns_path),
                items: None,
            };

            let mut index_file = File::create(format!("docs/{}/index.md", &ns_path)).await?;

            index_file
                .write_all(format!("# {}\n", ns_name).as_bytes())
                .await?;

            index_file.write_all(description.as_bytes()).await?;

            p = (book_id, PathBuf::from(format!("docs/{}/", ns_path)));
        }

        Ok((navbar_item, p))
    }

    pub async fn generate_all(&self) -> Result<()> {
        let mut default_navbar = vec![];
        let mut ns_id_paths = vec![];
        let mut id_ns = vec![];
        let mut custom_navbar_map = HashMap::new();
        let mut custom_navbar_list = vec![];

        for namespace in self.inner.read().await.namespaces.iter() {
            let (n, p) = self.generate_one(namespace).await?;
            id_ns.push((p.0, namespace.clone()));
            ns_id_paths.push(p);

            if namespace.nav.is_empty() {
                default_navbar.push(n);
            } else if namespace.nav == "true" {
                custom_navbar_list.push(n);
            } else if custom_navbar_map.contains_key(&namespace.nav) {
                custom_navbar_map.entry(namespace.nav.clone()).and_modify(
                    |items: &mut Vec<NavbarItem>| {
                        items.push(n);
                    },
                );
            } else {
                custom_navbar_map.insert(namespace.nav.clone(), vec![n]);
            }
        }

        generate_doc_sidebar("./docs")?;

        self.inner.write().await.ns_id_path.extend(ns_id_paths);
        self.inner.write().await.id_ns.extend(id_ns);

        let mut navbar = vec![];

        for (k, v) in custom_navbar_map {
            navbar.push(serde_json::json!({
                "text": k,
                "items": v
            }))
        }

        for v in custom_navbar_list {
            navbar.push(v.into())
        }

        if !default_navbar.is_empty() {
            navbar.push(serde_json::json!({
                "text": "知识库",
                "items": default_navbar,
            }));
        }

        File::create("./nav.json")
            .await?
            .write_all(serde_json::to_string(&navbar)?.as_bytes())
            .await?;

        info!("Generate navbar config.");

        let mut schemas = serde_json::json!({});
        schemas["首页介绍"] = serde_json::json!([]);
        schemas["首页链接"] = serde_json::json!([]);

        self.schemas.lock().unwrap().iter_mut().for_each(|(k, v)| {
            // schemas[k] = v.to_owned();
            let v = v.as_object_mut().unwrap();
            if let Some(v) = v.remove("首页介绍") {
                schemas["首页介绍"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!({
                        "content": v,
                        "link": k,
                    }));
            }

            if let Some(v) = v.remove("首页链接") {
                schemas["首页链接"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!({
                        "content": v,
                        "link": k,
                    }));
            }

            if !v.is_empty() {
                warn!(
                    "Schema `{}` has unused keys: {:?}",
                    k,
                    v.keys().collect::<Vec<_>>()
                );
            }
        });

        File::create("./schema.json")
            .await?
            .write_all(serde_json::to_string_pretty(&schemas)?.as_bytes())
            .await?;

        info!("Generate markdown schema.");

        Ok(())
    }

    pub async fn build(&self) -> Result<()> {
        let inner = self.inner.read().await;

        let cmd = &inner.build_command;

        let (program, args) = inner
            .build_command
            .split_once(' ')
            .ok_or_else(|| Error::InvalidBuildCommand(cmd.to_string()))?;

        let args = args.split(' ').collect::<Vec<_>>();

        info!("Use `{}` to build.", cmd);

        if run_display_command_output(program, &args, 0, 3).await {
            info!("Build Finished.");
        } else {
            error!("Build Failed.");
        }

        Ok(())
    }

    pub async fn clean(&self, book_id: i32) -> Result<()> {
        let inner = self.inner.read().await;
        let path = inner.ns_id_path.get(&book_id).unwrap();
        warn!("removing dir: {}", path.display());
        remove_dir_all(path).await?;

        Ok(())
    }

    pub async fn regenerate(&self, book_id: i32) -> Result<()> {
        self.clean(book_id).await?;

        if let Some(ns) = self.inner.read().await.id_ns.get(&book_id) {
            info!("Regenerate repos: {}", ns.target);
            self.generate_one(ns).await?;
        }

        generate_doc_sidebar("./docs")?;

        Ok(())
    }

    async fn write_markdown_with_toc(
        &self,
        client: &DocsClient,
        path: PathBuf,
        ns: &str,
        doc: Toc<'_>,
        order: usize,
    ) -> Result<()> {
        match doc {
            Toc::Doc(doc) => {
                self.write_markdown(client, path, ns, doc.id, order).await?;
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
        &self,
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

        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .unwrap();

        let article_path = self.article_path.read().await;
        let default_map = HashMap::default();

        let articles = article_path.get(ns).unwrap_or(&default_map);

        let mut formatter = Formatter::new();

        let mut ap = path.components();
        ap.next();
        ap.next();

        let content = filter_schema(&doc.body, ap.collect::<PathBuf>().as_path(), &self.schemas);

        formatter
            .parse(&content)
            .format_with_args(convert_image_to_base64, &client)
            .format_with_args(convert_link, articles)
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

fn image_to_base64(img: &DynamicImage) -> String {
    let mut image_data: Vec<u8> = Vec::new();
    img.write_to(&mut Cursor::new(&mut image_data), ImageOutputFormat::Png)
        .unwrap();

    info!("Convert image to base64 string.");

    let res_base64 = base64::prelude::BASE64_STANDARD.encode(image_data);
    format!("data:image/png;base64,{}", res_base64)
}

fn convert_image_to_base64<'a>(
    node: &'a AstNode<'a>,
    client: &reqwest::blocking::Client,
) -> Result<()> {
    let mut svg = vec![];

    if let NodeValue::Image(i) = &mut node.data.borrow_mut().value {
        let url = String::from_utf8_lossy(&i.url).to_string();
        let url = url::Url::parse(&url)?;

        info!("Find image url: {}", url);

        let response = client.get(url).send()?;

        let bytes = response.bytes()?;

        if bytes.starts_with(b"<svg") {
            svg = bytes.into();
        } else {
            i.url = image_to_base64(&image::load_from_memory(&bytes)?).into_bytes();
        }
    }

    if svg.is_empty() {
        return Ok(());
    }

    node.data.borrow_mut().value = NodeValue::HtmlBlock(NodeHtmlBlock {
        block_type: 7,
        literal: svg,
    });

    Ok(())
}

fn convert_link<'a>(node: &'a AstNode<'a>, articles: &HashMap<String, PathBuf>) -> Result<()> {
    let mut url = String::new();
    let mut content = None;

    if let NodeValue::Link(link) = &node.data.borrow().value {
        url = String::from_utf8_lossy(&link.url).to_string();
        content = node.first_child();
    }

    let origin_url = url::Url::parse(&url)?;

    let domain = origin_url
        .domain()
        .ok_or(Error::Internal(format!("Invalid url: {}", url)))?;

    // Codepen
    if domain.contains("codepen") {
        let literal = CODEPEN_IFRAME.replace("{}", &url).into_bytes();
        node.children().for_each(|node| node.detach());

        let mut data = node.data.borrow_mut();
        data.value = NodeValue::HtmlBlock(NodeHtmlBlock {
            block_type: 6,
            literal,
        });

        return Ok(());
    }

    // inner link
    let path = PathBuf::from(origin_url.path());

    let doc_slug = path
        .file_name()
        .ok_or(Error::Internal(format!("Invalid url path: {}", url)))?;
    let doc_slug = doc_slug.to_str().unwrap().to_string();

    let path = articles
        .get(&doc_slug)
        .ok_or(Error::Internal(format!("No such document: {}", doc_slug)))?;

    let path = path.strip_prefix("./docs").unwrap().display();
    info!("change url to inner link: {}", path);
    node.children().for_each(|node| node.detach());

    let mut data = node.data.borrow_mut();
    data.value = NodeValue::Link(NodeLink {
        url: format!("/{}", path).into_bytes(),
        title: vec![],
    });

    if let Some(content) = content {
        node.append(content);
    }

    Ok(())
}

#[allow(unused)]
fn parse_schema_start_line<'a>(
    node: &'a AstNode<'a>,
    schema_start_line: &RefCell<u32>,
) -> Result<()> {
    let node_data = node.data.borrow();

    if let NodeValue::HtmlBlock(html) = &node_data.value {
        if !html.literal.starts_with(b"<hr") {
            return Ok(());
        }
    }

    let key_node = node
        .next_sibling()
        .ok_or(Error::InvalidSchema("expected attribution".into()))?;

    let key_node_value = &key_node.data.borrow().value;

    if let NodeValue::Heading(NodeHeading { level, .. }) = key_node_value {
        if *level != 2 {
            return Ok(());
        }
    }

    key_node
        .first_child()
        .ok_or(Error::InvalidSchema("expected attribution".into()))?;

    key_node
        .next_sibling()
        .ok_or(Error::InvalidSchema("expected value".into()))?;

    *schema_start_line.borrow_mut() = key_node.data.borrow().start_line;
    Ok(())
}

fn parse_schema(text: &str) -> serde_json::Value {
    let mut current_key = "";

    let lines = text.lines();
    let mut schema = serde_json::json!({});
    let anchor = Regex::new("<a .*>").unwrap();

    for line in lines {
        if anchor.is_match(line) || line.is_empty() {
            continue;
        }

        if line.starts_with("##") {
            current_key = line.trim_start_matches("##").trim();
            schema[current_key] = serde_json::json!([]);
            continue;
        }

        if !current_key.is_empty() {
            schema[current_key]
                .as_array_mut()
                .unwrap()
                .push(line.into());
            current_key = "";
        }
    }

    schema
}

fn filter_schema(text: &str, path: &Path, schemas: &Mutex<HashMap<String, Value>>) -> String {
    info!("filter schema: {}", path.display());
    let schemas = &mut schemas.lock().unwrap();

    let regex = Regex::new(r"---").unwrap();

    let mut split_result: Vec<_> = regex.split(text).collect();

    if split_result.len() == 1 {
        split_result.pop().unwrap().to_owned()
    } else {
        let schema_string = split_result.pop().unwrap();

        let content_string = split_result;

        let schema = parse_schema(schema_string);
        schemas.insert(path.display().to_string(), schema);
        content_string.join("---")
    }
}
