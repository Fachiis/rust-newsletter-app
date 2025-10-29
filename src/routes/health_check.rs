use actix_web::{web, HttpResponse};
use sqlx::PgPool;

#[tracing::instrument(name = "Database health check", skip(pool))]
pub async fn health_check(pool: web::Data<PgPool>) -> HttpResponse {
    // HttpResponse::Ok().finish()
    match sqlx::query!("SELECT 1 as check")
        .fetch_one(pool.get_ref())
        .await
    {
        Ok(_) => {
            tracing::info!("Database connection successful");
            HttpResponse::Ok().finish()
        }
        Err(e) => {
            tracing::error!("Database connection failed: {:?}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}
