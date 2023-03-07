mod initialize {
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

    use std::ops::Not;
    use std::thread::sleep;
    use std::time::Duration;
    use std::{
        fs::{rename, File},
        io::Write,
        path::Path,
        process::{exit, Command},
    };

    use log::{debug, info, warn};
    use serde_json::Value;

    use crate::config::{Check, CheckedGeneratorConfig, CheckedSiteConfig};
    use crate::{
        config::Config,
        error::{Error, Result},
    };

    const DEFAULT_JSON: &[u8] = br#"{ "name": "yuque-ssg", "version": "1.0.0", "description": "", "main": "index.js", "scripts": {  "docs:dev": "vitepress dev docs",  "docs:build": "vitepress build docs", "docs:preview": "vitepress preview docs" }, "keywords": [], "author": "", "license": "ISC", "devDependencies": { "vitepress": "1.0.0-alpha.49", "vue": "^3.2.47" }}"#;

    impl<'a> Config<'a> {
        pub fn read_config(
            path: impl AsRef<Path>,
        ) -> Result<(CheckedSiteConfig<'a>, CheckedGeneratorConfig<'a>)> {
            let config_file = File::open(path)?;

            info!("Read config from: `config.yml`");

            let config: Config = serde_yaml::from_reader(config_file)?;

            debug!("Config: {:#?}", config);

            info!("Check config.");

            config.check()
        }
    }

    impl<'a> CheckedSiteConfig<'a> {
        pub fn check_env(self) -> Result<Self> {
            info!("Checking node.");

            Command::new("node")
                .arg("-v")
                .output()
                .map_err(|_| Error::MissingEnv("node".into()))?;

            info!("Checking `node.js` package manager.");
            let pnpm = Command::new("pnpm").arg("-v").output().is_ok();
            let yarn = Command::new("yarn").arg("-v").output().is_ok();
            let npm = Command::new("npm").arg("-v").output().is_ok();

            (pnpm || yarn || npm)
                .then_some(true)
                .ok_or(Error::MissingEnv("npm".into()))?;

            info!("Checking `package.json`.");
            if Path::new("package.json").exists() {
                info!("Find existed `package.json`, checking.");
                let json_file = File::open("package.json")?;

                let json = serde_json::from_reader::<File, Value>(json_file)?;

                let json = json
                    .as_object()
                    .ok_or(Error::CantParse("package.json".into()))?;

                let dep = json
                    .get("devDependencies")
                    .ok_or(Error::MissingDependency("vue, vitepress".into()))?;

                let dep = dep
                    .as_object()
                    .ok_or(Error::CantParse("package.json".into()))?;

                dep.get("vue")
                    .ok_or(Error::MissingDependency("vue".into()))?;
                dep.get("vitepress")
                    .ok_or(Error::MissingDependency("vitepress".into()))?;
            } else {
                info!("Can not find existed `package.json` in current directory, write a default version.");
                File::create("package.json")?.write_all(DEFAULT_JSON)?;
            }

            match (pnpm, yarn, npm) {
                (true, _, _) => Self::install_dependencies("pnpm")?,
                (_, true, _) => Self::install_dependencies("yarn")?,
                (_, _, true) => Self::install_dependencies("npm")?,
                _ => Err(Error::MissingEnv("npm".into()))?,
            }

            Ok(self)
        }

        pub fn generate_config_js(self) -> Result<()> {
            info!("Generating `config.js`");

            let file_path = Path::new("./docs/.vitepress/config.js");

            if file_path.exists() {
                info!("Find existed `config.js`, rename it to `config.old.js`");
                let new_file_path = file_path.parent().unwrap().join("config.old.js");
                rename(file_path, new_file_path)?;
            }

            if file_path.parent().unwrap().exists().not() {
                std::fs::create_dir_all(file_path.parent().unwrap())?;
            }

            let mut js_file = File::create(file_path)?;

            let CheckedSiteConfig {
                title,
                lang,
                description,
                base,
            } = self;

            let content = format!(
                r#"import {{ defineConfig }} from 'vitepress'
import nav from '../../nav.json'
import sidebar from '../../sidebar.json'

export default defineConfig({{
    themeConfig: {{
        "nav": [nav,],
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
                description.unwrap_or_default(),
            );

            js_file.write_all(content.as_bytes())?;

            Ok(())
        }

        fn install_dependencies(program: &str) -> Result<()> {
            info!("use `{}`", program);

            let mut failed_count: u8 = 0;

            while failed_count < 3 {
                info!("Install the dependencies.");

                let command = Command::new(program).arg("install").output()?;

                if command.status.success() {
                    break;
                } else {
                    warn!("Install dependencies failed.");
                    warn!("{}", String::from_utf8(command.stderr).unwrap());
                    if failed_count < 2 {
                        warn!("Will retry after 3s.");
                    }
                    failed_count += 1;

                    sleep(Duration::from_secs(3));
                }
            }

            if failed_count == 3 {
                warn!("Install dependencies failed.");
                warn!("Exiting.");
                exit(1);
            }
            Ok(())
        }
    }
}
