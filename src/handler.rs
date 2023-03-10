use actix_files::Files;

use actix_web::{
    post,
    web::{Data, Json},
    HttpResponse, Responder,
};

use serde::Deserialize;

use tokio::sync::{Notify, RwLock};

#[post("/webhook")]
pub async fn webhook(
    data: Json<WebhookData>,
    rebuild: Data<Notify>,
    info: Data<RwLock<i32>>,
) -> impl Responder {
    *info.write().await = data.data.book_id;
    rebuild.notify_one();

    HttpResponse::Ok()
}

pub fn static_file(mount_path: &str) -> Files {
    actix_files::Files::new(mount_path, "docs/.vitepress/dist").index_file("index.html")
}

#[derive(Debug, Deserialize)]
pub struct WebhookData {
    pub data: WebhookDataDetail,
}

#[derive(Debug, Deserialize)]
pub struct WebhookDataDetail {
    pub book_id: i32,
}
