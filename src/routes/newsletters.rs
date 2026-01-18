use actix_web::{web, HttpResponse};

// Newsletter request body data. We derive Deserialize to parse the incoming request body. parsing is done automatically by Actix Web. (parsing means converting the raw HTTP request body into a Rust data structure)
#[derive(serde::Deserialize)]
pub struct BodyData {
    title: String,
    content: Content,
}

#[derive(serde::Deserialize)]
pub struct Content {
    text: String,
    html: String,
}

// We will now use an extractor to parse the incoming request body into our BodyData struct
pub async fn publish_newstter(_body: web::Json<BodyData>) -> HttpResponse {
    HttpResponse::Ok().finish()
}
