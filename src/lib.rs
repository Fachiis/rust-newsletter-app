use std::net::TcpListener;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use actix_web::dev::Server;

// Responder is a trait for types that can be converted to an HTTP response.
// async fn greet(req: HttpRequest) -> impl Responder {
//     let name = req.match_info().get("name").unwrap_or("World");
//     format!("Hello, {}!", name)
// }

async fn health_check() -> impl Responder {
    HttpResponse::Ok()
}


pub fn run(listener: TcpListener) -> Result<Server, std::io::Error> {
    let server = HttpServer::new(|| {
        App::new()
            .route("/health_check", web::get().to(health_check))
    })
        .listen(listener)?
        .run();

    Ok(server)
}