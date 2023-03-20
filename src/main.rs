use actix_files::NamedFile;
use actix_web::{
    dev::{fn_service, ServiceRequest, ServiceResponse},
    middleware::Logger,
    App, HttpServer,
};

use yuque_ssg::{
    handler::{static_file, webhook},
    init::initialize,
    log::init_logger,
};

use std::error::Error;

#[actix_web::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_logger();

    let ((rebuild, rebuild_info), config) = initialize().await?;

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::new("%r %s"))
            .app_data(rebuild.clone())
            .app_data(rebuild_info.clone())
            .service(webhook)
            .service(static_file("/"))
            .default_service(fn_service(|req: ServiceRequest| async {
                let (req, _) = req.into_parts();
                let file = NamedFile::open_async("docs/.vitepress/dist/404.html").await?;
                let res = file.into_response(&req);
                Ok(ServiceResponse::new(req, res))
            }))
    })
    .bind(format!("{}:{}", config.host, config.port))?
    .run()
    .await?;

    Ok(())
}
