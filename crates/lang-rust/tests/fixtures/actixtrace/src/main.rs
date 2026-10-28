use actix_web::route;

#[route("/x", method = "TRACE")]
async fn x() {}

fn main() {}
