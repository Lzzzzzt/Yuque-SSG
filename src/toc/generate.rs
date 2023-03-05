//! 为每个知识库生成目录树，以便在前端显示。

use std::{
    collections::HashMap,
    fs::{self, File},
    path::{Path, PathBuf},
};

use log::{debug, info};

use super::{Frontmatter, NavbarItem};

pub fn generate_doc_sidebar(doc_dir: impl AsRef<Path>) -> anyhow::Result<()> {
    info!(
        "Walking the `{}` to generate sidebar.json",
        doc_dir.as_ref().display()
    );

    let doc_dir = fs::read_dir(doc_dir)?;

    let mut sidebar_json = HashMap::new();

    for file in doc_dir {
        let file = file?;

        let file_name = file.file_name();
        let file_name = file_name.to_string_lossy();
        let file_type = file.file_type()?;

        debug!("Find file: {}", file_name);

        if file_type.is_dir() && !file_name.starts_with('.') && !file_name.starts_with('_') {
            let mut json = walk(file.path(), file_name.to_string())?;
            json.sort_by_key(|v| v.order);
            let name = format!("/{}/", file_name.to_lowercase());
            info!("Generate sidebar config for {}", name);
            sidebar_json.insert(name, json);
        }
    }

    serde_json::to_writer_pretty(File::create("sidebar.json")?, &sidebar_json)?;

    Ok(())
}

fn walk(dir: impl AsRef<Path>, base: impl AsRef<Path>) -> anyhow::Result<Vec<NavbarItem>> {
    let path = if PathBuf::from(base.as_ref()).is_absolute() {
        PathBuf::from(base.as_ref())
    } else {
        PathBuf::from("/").join(base.as_ref())
    };

    let directory = fs::read_dir(&dir)?;

    debug!("Find a document root: {}", dir.as_ref().display());

    let mut result = vec![];

    for file in directory {
        let file = file?;

        let file_name = file.file_name();
        let file_name = file_name.to_string_lossy();
        let file_type = file.file_type()?;

        if file_name.starts_with("index") {
            continue;
        }

        let mut children = vec![];

        let mut item_builder = NavbarItem::builder();

        if file_type.is_dir() {
            let Frontmatter { order, sidebar, .. } =
                Frontmatter::from_file(file.path().join("index.md"))?;

            // let text = BufReader::new(File::open(file.path().join("index.md")).unwrap())
            //     .lines()
            //     .next()
            //     .unwrap()?;
            // let text = sidebar.strip_prefix('#').unwrap().trim();

            item_builder.text(sidebar.into()).order(order);

            let mut items = walk(file.path(), path.join(file_name.to_string()))?;

            items.sort_by_key(|v| v.order);

            children.append(&mut items);

            item_builder.link(
                path.join(format!("{}/", file_name))
                    .display()
                    .to_string()
                    .to_lowercase(),
            );

            item_builder.items(Some(children));
            item_builder.collapsed(Some(false));
        } else if file_type.is_file() {
            let Frontmatter { order, sidebar, .. } = Frontmatter::from_file(file.path())?;

            // item_builder.text(text.to_string()).order(order);
            //     let text = BufReader::new(File::open(file.path()).unwrap())
            //         .lines()
            //         .next()
            //         .unwrap()?;
            // let text = sidebar.strip_prefix('#').unwrap().trim();

            item_builder.text(sidebar.into()).order(order);

            item_builder.link(
                path.join(format!("{}.html", file_name.split_once('.').unwrap().0))
                    .display()
                    .to_string()
                    .to_lowercase(),
            );
        }

        result.push(item_builder.build()?);
    }

    Ok(result)
}
