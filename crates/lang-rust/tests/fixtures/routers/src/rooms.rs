use axum::Router;

use crate::bookings;

pub fn router() -> Router {
    bookings::router()
}
