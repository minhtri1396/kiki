//! Pattern: Controller. Each area of the API (agents, indexes, settings, tasks, ...) owns a `XController`
//! that registers its routes; `Server` only builds the router out of them and dispatches requests.
//!
//! A controller handler stays thin: read the request, call a service method on [`Server`], map the result to a
//! reply. Register fallible handlers with [`Router::api`]: they return [`ApiResult`] and use `?`.

use crate::http::Router;
use crate::Server;

/// An area of the API that registers its own routes.
pub trait Controller {
    /// Adds every route of this area to `router`.
    fn register(router: &mut Router<Server>);
}

/// Builds the router with the routes of every controller.
pub fn build_router() -> Router<Server> {
    use crate::atlassian_list::AtlassianController;
    use crate::clone::CloneController;
    use crate::groups::GroupController;
    use crate::guard::GuardController;
    use crate::runs::RunController;
    use crate::server::controllers::*;
    use crate::tasks::TaskController;
    use crate::team::TeamController;

    let mut router = Router::new();
    StateController::register(&mut router);
    SettingsController::register(&mut router);
    AgentController::register(&mut router);
    IndexController::register(&mut router);
    JobController::register(&mut router);
    GenerateController::register(&mut router);
    ModelController::register(&mut router);
    AtlassianController::register(&mut router);
    CloneController::register(&mut router);
    GroupController::register(&mut router);
    TaskController::register(&mut router);
    RunController::register(&mut router);
    GuardController::register(&mut router);
    TeamController::register(&mut router);
    router
}
