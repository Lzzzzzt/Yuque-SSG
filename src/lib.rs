pub mod config;
pub mod error;
pub mod generator;
pub mod handler;
pub mod init;
pub mod log;
pub mod toc;

mod archive;

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 6.1; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/31.0.1650.63 Safari/537.36";
pub const CODEPEN_IFRAME: &str = r#"<iframe height="400" style="width: 100%;" scrolling="no" title="Untitled" src="{}" frameborder="no" loading="lazy" allowtransparency="true" allowfullscreen="true"></iframe>
"#;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ::log::{debug, info, warn};
use futures_util::future::{BoxFuture, FutureExt};
use tokio::{process::Command, time::sleep};

use error::Result;

pub fn run_display_command_output<'a>(
    program: &'a str,
    args: &'a [&'a str],
    retry: u8,
    max: u8,
) -> BoxFuture<'a, bool> {
    if retry > 0 && retry <= max {
        warn!("Retry {} times", retry);
    }
    if retry > max {
        return async { false }.boxed();
    }

    async move {
        match Command::new(program).args(args).output().await {
            Ok(output) => {
                if !output.status.success() {
                    warn!("Run command `{} {}` failed. ", program, args.join(" "));

                    for line in String::from_utf8_lossy(&output.stderr).lines() {
                        warn!("{}", line.trim());
                    }
                    for line in String::from_utf8_lossy(&output.stdout).lines() {
                        warn!("{}", line.trim());
                    }

                    if retry < 3 && max != 0 {
                        warn!("Retry after 3s.");
                    }
                    sleep(Duration::from_secs(3)).await;
                    run_display_command_output(program, args, retry + 1, max).await
                } else {
                    for line in String::from_utf8_lossy(&output.stdout).lines() {
                        info!("{}", line.trim());
                    }

                    true
                }
            }
            Err(e) => {
                warn!("Run command `{} {}` failed. ", program, args.join(" "));
                warn!("{}", e);
                if retry < 3 {
                    warn!("Retry after 3s.");
                }
                sleep(Duration::from_secs(3)).await;
                run_display_command_output(program, args, retry + 1, max).await
            }
        }
    }
    .boxed()
}

pub fn copy<U: AsRef<Path>, V: AsRef<Path>>(from: U, to: V) -> Result<(), std::io::Error> {
    let mut stack = Vec::new();
    stack.push(PathBuf::from(from.as_ref()));

    let output_root = PathBuf::from(to.as_ref());
    let input_root = PathBuf::from(from.as_ref()).components().count();

    while let Some(working_path) = stack.pop() {
        debug!("process: {:?}", &working_path);

        // Generate a relative path
        let src: PathBuf = working_path.components().skip(input_root).collect();

        // Create a destination if missing
        let dest = if src.components().count() == 0 {
            output_root.clone()
        } else {
            output_root.join(&src)
        };
        if fs::metadata(&dest).is_err() {
            debug!(" mkdir: {:?}", dest);
            fs::create_dir_all(&dest)?;
        }

        for entry in fs::read_dir(working_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                match path.file_name() {
                    Some(filename) => {
                        let dest_path = dest.join(filename);
                        debug!("  copy: {:?} -> {:?}", &path, &dest_path);
                        fs::copy(&path, &dest_path)?;
                    }
                    None => {
                        debug!("failed: {:?}", path);
                    }
                }
            }
        }
    }

    Ok(())
}
