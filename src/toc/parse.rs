//! 解析从语雀获取的目录结构

use std::{ops::Not, path::PathBuf};

use pinyin::ToPinyin;
use yuque_rust::Toc;

pub fn parse_toc_structure(root: &str, toc: &[Toc]) -> Vec<PathBuf> {
    let mut path = PathBuf::from(format!("./docs/{}", root));

    let mut level = 0;
    let mut is_index = false;
    let mut result = vec![];

    for item in toc {
        if let Toc::Doc(item) = item {
            if level > item.level {
                while level != item.level {
                    path.pop();
                    level -= 1;
                }
            }

            if item.child_uuid.is_empty().not() {
                level += 1;
                path = path.join(item.title.to_string().to_pinyin_or_lowercase());

                is_index = true;
            }

            if is_index {
                result.push(path.join("index.md"));
            } else {
                result.push(path.join(format!("{}.md", item.title.to_pinyin_or_lowercase())));
            }

            is_index = false;
        } else if let Toc::Title(item) = item {
            if level > item.level {
                while level != item.level {
                    path.pop();
                    level -= 1;
                }
            }

            if item.child_uuid.is_empty().not() {
                level += 1;
                path = path.join(item.title.to_string().to_pinyin_or_lowercase());
            }

            result.push(path.clone());
        }
    }

    result
}

trait Pinyin
where
    Self: ToString,
{
    fn to_pinyin_or_lowercase(&self) -> String;
}

impl<T> Pinyin for T
where
    T: ToString,
{
    fn to_pinyin_or_lowercase(&self) -> String {
        let hans = regex::Regex::new(r"[\u4e00-\u9fa5]").unwrap();

        let mut current = String::new();

        let mut result = vec![];

        for c in self.to_string().chars() {
            if hans.is_match(&c.to_string()) {
                if !current.is_empty() {
                    result.push(current.clone());
                    current.clear();
                }
                result.push(c.to_pinyin().unwrap().plain().to_string())
            } else {
                current.push(c);
            }
        }

        if result.is_empty() {
            result.push(self.to_string());
        }

        let regex = regex::Regex::new(r"([A-Z])").unwrap();

        result.iter_mut().for_each(|s| {
            let str = s.to_string();

            let result = regex.replace_all(&str, r"-$0");

            *s = if let Some(v) = result.strip_prefix('-') {
                v.to_lowercase()
            } else {
                result.to_string()
            }
        });

        result.join("-")
    }
}
