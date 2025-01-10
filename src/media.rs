use crate::state::AppState;
use actix_files::Files;
use actix_web::{
    web::{self, Redirect},
    Responder, Scope,
};
use mitra_services::media::{FilesystemStorage, MediaStorage, MEDIA_ROOT_URL};

async fn s3_media_server_handler(
    state: web::Data<AppState>,
    filename: web::Path<String>,
) -> impl Responder {
    if let MediaStorage::S3(s3) = &state.media_storage {
        let url = s3.presign_url(&filename).unwrap();
        Redirect::to(url)
    } else {
        panic!("Media storage is not S3");
    }
}

pub fn s3_media_server() -> Scope {
    web::scope(MEDIA_ROOT_URL).route("/{filename}", web::get().to(s3_media_server_handler))
}

pub fn filesystem_media_server(backend: FilesystemStorage) -> Files {
    Files::new(MEDIA_ROOT_URL, backend.media_dir.clone())
}
