use crate::email_client::EmailClient;
use crate::routes::{health_check, subscribe};
use actix_web::dev::Server;
use actix_web::{web, App, HttpServer};
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

pub fn run(
    listener: TcpListener,
    db_pool: PgPool,
    email_client: EmailClient,
) -> Result<Server, std::io::Error> {
    // web::Data is a smart pointer Arc<T> around the PgConnection that makes it possible to share
    // the connection across different workers of the application server.
    // With this, we have a cheap clone of the pointer instead of cloning the whole connection,
    // and only one connection is created for the whole application.
    let db_pool = web::Data::new(db_pool);
    let email_client = web::Data::new(email_client);

    // Beware: app instance is created for each worker thread -  the cost of a string allocation (or a pointer clone) is negligible compared to the cost of handling a request - so it's ok to clone the db_pool here
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .app_data(db_pool.clone()) // Register the DB connection as part of the application state: stateful remember of the DB connection
            .app_data(email_client.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
