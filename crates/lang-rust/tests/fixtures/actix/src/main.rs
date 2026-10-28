mod bookings;

use actix_web::{get, route, web, App, HttpServer};

#[get("/health")]
async fn health() {}

#[route("/ping", method = "GET", method = "HEAD")]
async fn ping() {}

async fn list() {}
async fn add() {}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(health)
            .service(ping)
            .service(
                web::resource("/rooms")
                    .route(web::get().to(list))
                    .route(web::post().to(add)),
            )
            .route("/status", web::get().to(health))
            .service(web::scope("/bookings").service(bookings::show))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
