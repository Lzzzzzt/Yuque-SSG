//! SSG 的初始化阶段
//! 1. 读取配置
//! 2. 检查环境是否准备完成
//!     1. 检查 pnpm/yarn/npm
//!         + 没有则退出流程
//!     2. 检查 package.json
//!         + 如果存在则检查其依赖是否符合要求
//!         + 如果不存在则写入默认的
//!     3. 安装依赖
//!         + 失败则重试3次
//! 3. 启动服务器 todo
//!

use std::path::{Path, PathBuf};

use actix_web::web::{self, Data};
use log::{debug, info, warn};
use tokio::fs;
use tokio::process::Command;
use tokio::sync::{Notify, RwLock};

use crate::config::{Check, CheckedGeneratorConfig, CheckedSiteConfig};
use crate::generator::Generator;
use crate::{
    config::Config,
    error::{Error, Result},
};
use crate::{copy, run_display_command_output};

// const DEFAULT_JSON: &[u8] = br#"{ "name": "yuque-ssg", "version": "1.0.0", "description": "", "main": "index.js", "scripts": {  "docs:dev": "vitepress dev docs",  "docs:build": "vitepress build docs", "docs:preview": "vitepress preview docs" }, "keywords": [], "author": "", "license": "ISC", "devDependencies": { "vitepress": "1.0.0-alpha.49", "vue": "^3.2.47" }}"#;

impl<'a> Config<'a> {
    pub fn read_config(
        path: impl AsRef<Path>,
    ) -> Result<(CheckedSiteConfig<'a>, CheckedGeneratorConfig<'a>)> {
        let config_file = std::fs::File::open(path)?;

        info!("Read config from: `config.yml`");

        let config: Config = serde_yaml::from_reader(config_file)?;

        debug!("Config: {:#?}", config);

        info!("Check config.");

        config.check()
    }
}

impl<'a> CheckedSiteConfig<'a> {
    pub async fn check_env(&self) -> Result<()> {
        info!("Checking `git`.");
        Command::new("git")
            .arg("-v")
            .output()
            .await
            .map_err(|_| Error::MissingEnv("git".into()))?;

        self.clone_theme().await?;

        info!("Checking `node`.");
        Command::new("node")
            .arg("-v")
            .output()
            .await
            .map_err(|_| Error::MissingEnv("node".into()))?;

        info!("Checking `node.js` package manager.");
        let pnpm = Command::new("pnpm").arg("-v").output().await.is_ok();
        let yarn = Command::new("yarn").arg("-v").output().await.is_ok();
        let npm = Command::new("npm").arg("-v").output().await.is_ok();

        (pnpm || yarn || npm)
            .then_some(true)
            .ok_or(Error::MissingEnv("npm".into()))?;

        // info!("Checking `package.json`.");
        // if Path::new("package.json").exists() {
        //     info!("Find existed `package.json`, checking.");
        //     let json_file = std::fs::File::open("package.json")?;

        //     let json = serde_json::from_reader::<std::fs::File, Value>(json_file)?;

        //     let json = json
        //         .as_object()
        //         .ok_or(Error::CantParse("package.json".into()))?;

        //     let dep = json
        //         .get("devDependencies")
        //         .ok_or(Error::MissingDependency("vue, vitepress".into()))?;

        //     let dep = dep
        //         .as_object()
        //         .ok_or(Error::CantParse("package.json".into()))?;

        //     dep.get("vue")
        //         .ok_or(Error::MissingDependency("vue".into()))?;
        //     dep.get("vitepress")
        //         .ok_or(Error::MissingDependency("vitepress".into()))?;
        // } else {
        //     info!("Can not find existed `package.json` in current directory, write a default version.");
        //     File::create("package.json")
        //         .await?
        //         .write_all(DEFAULT_JSON)
        //         .await?;
        // }

        match (pnpm, yarn, npm) {
            (true, _, _) => Self::install_dependencies("pnpm").await?,
            (_, true, _) => Self::install_dependencies("yarn").await?,
            (_, _, true) => Self::install_dependencies("npm").await?,
            _ => Err(Error::MissingEnv("npm".into()))?,
        }

        Ok(())
    }

    pub async fn clone_theme(&self) -> Result<()> {
        if fs::try_exists("./theme").await? {
            info!("Theme directory exists. Skipping clone the repo");
        } else {
            let theme_repo = &self.theme;
            let path = PathBuf::from(theme_repo.to_string());

            info!("Theme repo: {}", path.display());

            if path.is_dir() {
                info!("Theme repo is a local directory. Copying it.");
                copy(path, "./")?;
                return Ok(());
            }

            info!("Cloning the theme repo into current directory.");
            if !run_display_command_output(
                "git",
                &["clone", theme_repo, "./theme", "--depth", "1"],
                0,
                1,
            )
            .await
            {
                warn!("Can not fetch theme repo");
                return Err(Error::CantFetchTheme);
            }
        }

        copy("./theme", "./")?;

        Ok(())
    }

    async fn install_dependencies(program: &str) -> Result<()> {
        info!("use `{}`", program);

        if !run_display_command_output(program, &["install"], 0, 3).await {
            warn!("Can not install the theme dependencies");
            return Err(Error::CantInstallDependency);
        }

        // let mut failed_count: u8 = 0;

        // while failed_count < 3 {
        //     info!("Install the dependencies.");

        //     let command = Command::new(program).arg("install").output().await?;

        //     if command.status.success() {
        //         break;
        //     } else {
        //         warn!("Install dependencies failed.");
        //         warn!("{}", String::from_utf8(command.stderr).unwrap());
        //         if failed_count < 2 {
        //             warn!("Will retry after 3s.");
        //         }
        //         failed_count += 1;

        //         sleep(Duration::from_secs(3)).await;
        //     }
        // }

        // if failed_count == 3 {
        //     warn!("Install dependencies failed.");
        //     warn!("Exiting.");
        //     exit(1);
        // }
        Ok(())
    }
}

pub async fn initialize<'a>() -> Result<((Data<Notify>, Data<RwLock<i32>>), CheckedSiteConfig<'a>)>
{
    let (site, gen) = Config::read_config("config.yml")?;

    let generator: Generator = gen.into();

    generator.generate_all().await?;

    site.check_env().await?;

    generator.build().await?;

    let rebuild = web::Data::new(Notify::new());
    let rebuild_info = web::Data::new(RwLock::new(0));
    let rebuild_cloned = rebuild.clone();
    let rebuild_info_cloned = rebuild_info.clone();

    tokio::spawn(async move {
        loop {
            rebuild.notified().await;
            let info = *rebuild_info.read().await;
            info!("Got rebuild info: {}", info);
            generator.regenerate(info).await.ok();
            // generator.generate().await.ok();
            generator.build().await.ok();
        }
    });

    Ok(((rebuild_cloned, rebuild_info_cloned), site))
}
