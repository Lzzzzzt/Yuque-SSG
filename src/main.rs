use log::{debug, info};
use yuque_ssg::{
    log::init_logger,
    toc::{generate::generate_doc_sidebar, Frontmatter},
};

use std::{error::Error, iter::zip, ops::Not, path::PathBuf};

use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};
use yuque_rust::{DocsClient, Yuque};
use yuque_ssg::toc::parse::parse_toc_structure;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_logger();

    // Configure
    let yuque = Yuque::builder()
        .host("https://lzzzt.yuque.com/api/v2".into())
        .token("OFg2CEeldHQjwAcq6ejnID2tOzSstNPZwxg9OhG5".into())
        .build()?;

    // Configure
    let namespace = "lzzzt/ssg";

    let repos = yuque.repos();
    let docs = yuque.docs();

    let response = repos.get(namespace, None).await?.data;

    let description = response.description.unwrap_or_default();
    let toc = response.toc.unwrap();

    let paths = parse_toc_structure(&response.name.to_lowercase(), &toc.toc);

    for (i, (path, item)) in zip(paths, toc.toc).enumerate() {
        write_markdown(&docs, path, namespace, item.id, i)
            .await
            .ok();
    }

    let mut index_file =
        File::create(format!("./docs/{}/index.md", &response.name.to_lowercase())).await?;

    index_file
        .write_all(format!("# {}\n", response.name).as_bytes())
        .await?;

    index_file.write_all(description.as_bytes()).await?;

    generate_doc_sidebar("./docs")?;

    Ok(())
}

async fn write_markdown(
    client: &DocsClient,
    path: PathBuf,
    ns: &str,
    doc: impl ToString,
    order: usize,
) -> anyhow::Result<()> {
    let doc = client
        .get_with_repo_ns(ns, doc, Some(&[("raw", "1")]))
        .await?
        .data;

    info!("Find doc: {}", doc.title);

    if path.parent().unwrap().exists().not() {
        fs::create_dir_all(path.parent().unwrap()).await?;
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

    Ok(())
}
