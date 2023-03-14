use std::borrow::Cow;
use std::collections::HashMap;

use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;
use std::{io::Write, iter::zip, ops::Not, path::PathBuf};

use base64::Engine;

use image::{DynamicImage, ImageOutputFormat};
use log::{debug, error, info, warn};
use tokio::fs::remove_dir_all;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};
use yuque_rust::{DocsClient, Toc, Yuque};

use crate::error::Error;
use crate::toc::parse::Pinyin;
use crate::{
    config::{CheckedGeneratorConfig, Namespace},
    error::Result,
    toc::{generate::generate_doc_sidebar, parse::parse_toc_structure, Frontmatter, NavbarItem},
};
use crate::{run_display_command_output, USER_AGENT};

pub struct Generator<'n> {
    inner: Arc<RwLock<GeneratorInner<'n>>>,
    pub article_path: RwLock<HashMap<String, HashMap<String, PathBuf>>>,
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

        let link_regex =
            regex::Regex::new(r"(?P<pre>[^!])\[(?P<title>[^\]]*)\]\((?P<src>[^\)]+)\)").unwrap();

        let article_path = self.article_path.read().await;
        let default_map = HashMap::default();

        let articles = article_path.get(ns).unwrap_or(&default_map);

        let result = link_regex.replace_all(&doc.body, |capture: &regex::Captures| {
            let src = capture.name("src").unwrap().as_str();
            let title = capture.name("title").unwrap().as_str();
            let pre = capture.name("pre").unwrap().as_str();

            let url = url::Url::parse(src).unwrap();
            info!("Find link: {}", url);

            let path = PathBuf::from(url.path());

            if let Some(doc_slug) = path.file_name() {
                let doc_slug = doc_slug.to_str().unwrap().to_string();

                if let Some(path) = articles.get(&doc_slug) {
                    let path = path.strip_prefix("./docs").unwrap().display();
                    info!("change url to inner link: {}", path);
                    return format!("{}[{}](/{})", pre, title, path);
                }
            }

            format!("{}[{}]({})", pre, title, src)
        });

        let image_regex = regex::Regex::new(r"!\[(?P<title>[^\]]*)\]\((?P<src>[^\)]+)\)").unwrap();

        let result = image_regex.replace_all(&result, |capture: &regex::Captures| {
            let src = capture.name("src").unwrap().as_str();

            let url = url::Url::parse(src).unwrap();
            info!("Find image url: {}", url);

            let response = client.get(url).send().unwrap();

            let bytes = response.bytes().unwrap();

            if bytes.starts_with(b"<svg") {
                String::from_utf8_lossy(&bytes).lines().collect()
            } else {
                let image = image::load_from_memory(&bytes).unwrap();
                let bs = image_to_base64(&image);
                format!("![{}]({})", capture.name("title").unwrap().as_str(), bs)
            }
        });

        file.write_all(result.as_bytes())?;

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
