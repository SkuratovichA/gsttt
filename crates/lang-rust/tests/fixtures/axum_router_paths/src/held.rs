use axum::routing::{get, Router};

pub struct App {
    router: Router,
}

impl App {
    pub fn mount(self) -> Router {
        self.router.route("/held", get(|| async {}))
    }
}
