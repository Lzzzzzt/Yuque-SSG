use std::{borrow::Cow, iter::zip, ops::Not, path::PathBuf};

use log::{debug, info};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};
use yuque_rust::{DocsClient, Toc, Yuque};

use crate::{
    config::{GeneratorConfig, Namespace},
    error::Result,
    toc::{generate::generate_doc_sidebar, parse::parse_toc_structure, Frontmatter, NavbarItem},
};

#[allow(unused)]
pub struct Generator<'n> {
    client: Yuque,
    base: Cow<'n, str>,
    namespaces: Vec<Namespace<'n>>,
}

impl<'n> Generator<'n> {
    pub fn from_config(config: GeneratorConfig<'n>) -> Self {
        let GeneratorConfig {
            host,
            token,
            base,
            namespaces,
        } = config;

        let client = Yuque::builder()
            .host(host.into())
            .token(token.into())
            .build()
            .unwrap();

        Self {
            client,
            namespaces,
            base,
        }
    }

    pub async fn generate(&self) -> Result<()> {
        let mut navbar = vec![];

        for namespace in self.namespaces.iter() {
            let name = &namespace.target;
            let text = &namespace.text;
            let toc = namespace.toc;

            if toc {
                let repos = self.client.repos();
                let docs = self.client.docs();

                let response = repos.get(name, None).await?.data;

                let description = response.description.unwrap_or_default();
                let mut toc = response.toc.unwrap();

                toc.remove(0);

                let ns_path = &response.name.to_lowercase();

                let paths = parse_toc_structure(ns_path, &toc);

                for (i, (path, item)) in zip(paths, toc).enumerate() {
                    Self::write_markdown(&docs, path, name, item, i).await?;
                }

                navbar.push(NavbarItem {
                    text: text.to_string(),
                    link: format!("/{}/", ns_path),
                });

                let mut index_file = File::create(format!("./docs/{}/index.md", &ns_path)).await?;

                index_file
                    .write_all(format!("# {}\n", response.name).as_bytes())
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

    async fn write_markdown(
        client: &DocsClient,
        path: PathBuf,
        ns: &str,
        doc: Toc<'_>,
        order: usize,
    ) -> Result<()> {
        match doc {
            Toc::Doc(doc) => {
                let doc = client
                    .get_with_repo_ns(ns, doc.id, Some(&[("raw", "1")]))
                    .await?
                    .data;

                info!("Find doc: {}", doc.title);

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

                let mut async_file = File::from_std(file);

                async_file
                    .write_all(format!("# {}\n", doc.title).as_bytes())
                    .await?;

                async_file.write_all(doc.body.as_bytes()).await?;

                debug!("Write File to: {}", path.display());
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
}

impl<'a> From<GeneratorConfig<'a>> for Generator<'a> {
    fn from(value: GeneratorConfig<'a>) -> Self {
        Self::from_config(value)
    }
}
