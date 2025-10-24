use crate::routes::{health_check, subscribe};
use actix_web::dev::Server;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};
use sqlx::PgPool;
use std::net::TcpListener;

pub fn run(listener: TcpListener, db_pool: PgPool) -> Result<Server, std::io::Error> {
    // web::Data is a smart pointer Arc<T> around the PgConnection that makes it possible to share
    // the connection across different workers of the application server.
    let db_pool = web::Data::new(db_pool);

    // Capture the connection from the surrounding env to be arc shared across different workers of the machine
    let server = HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .app_data(db_pool.clone()) // Register the DB connection as part of the application state: stateful remember of the DB connection
    })
    .listen(listener)?
    .run();

    Ok(server)
}
