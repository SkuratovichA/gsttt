use actix_web::web;

const BOOKINGS: &str = "/bookings";

fn app() {
    web::resource(BOOKINGS).route(web::get().to(list));
    web::resource("/rooms").route(web::get().to(rooms));
}

fn main() {}
