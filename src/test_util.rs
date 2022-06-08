use crate::endpoint::Endpoint;
use serde::Serialize;
use std::string::FromUtf8Error;
use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

pub struct MockCredentials;

impl MockCredentials {
    pub const USERNAME: &'static str = "api_user";
    pub const PASSWORD: &'static str = "arandompassword";
    pub const API_KEY: &'static str = "arandomapikey";
}

#[derive(Serialize)]
pub struct MockAuthResponse {
    email: String,
    api_key: Option<String>,
    result: String,
}

impl MockAuthResponse {
    pub fn new() -> Self {
        Self {
            email: MockCredentials::USERNAME.to_owned(),
            api_key: Some(MockCredentials::API_KEY.to_owned()),
            result: "success".to_owned(),
        }
    }
}

pub async fn mock_server(response: ResponseTemplate, endpoint: &str) -> MockServer {
    let mock = Mock::given(matchers::method("POST"))
        .and(matchers::path(format!(
            "{}{}",
            Endpoint::BASE_API,
            endpoint
        )))
        .respond_with(response)
        .expect(1);

    let server = MockServer::start().await;
    server.register(mock).await;
    server
}

pub async fn body_as_string(server: &MockServer) -> Result<Option<String>, FromUtf8Error> {
    server
        .received_requests()
        .await
        .map(|mut v| v.pop())
        .flatten()
        .map(|r| String::from_utf8(r.body))
        .transpose()
}
