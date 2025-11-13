use actix_web::{
    post,
    web,
    App,
    HttpResponse,
    HttpServer,
    Responder,
};
use apx_sdk::{
    authentication::verify_portable_object,
};
use serde_json::{Value as JsonValue};

#[post("/outbox")]
async fn outbox(
    activity: web::Json<JsonValue>,
) -> impl Responder {
    match verify_portable_object(&activity) {
        Ok(_) => {
            println!("{activity}");
            HttpResponse::Accepted().finish()
        },
        Err(error) => {
            HttpResponse::UnprocessableEntity().body(error.to_string())
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new().service(outbox)
    })
    .bind(("127.0.0.1", 8380))?
    .run()
    .await
}
