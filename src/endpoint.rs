/// List of endpoints used by this library
pub struct Endpoint;

impl Endpoint {
    /// Base API path
    pub const BASE_API: &'static str = "/api/v1/";

    // AUTHORIZATION
    pub const FETCH_API_KEY: &'static str = "fetch_api_key";
    pub const FETCH_DEV_API_KEY: &'static str = "dev_fetch_api_key";

    // REAL-TIME EVENTS
    pub const REGISTER_EVENT_QUEUE: &'static str = "register";
    pub const EVENTS_QUEUE: &'static str = "events";
}
