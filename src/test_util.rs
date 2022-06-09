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

#[derive(Serialize)]
    pub struct MockErrorResponse {
        message: String,
        code: Option<String>,
        var_name: Option<String>,
        retry_after: Option<f32>,
    }

    impl MockErrorResponse {
        pub fn new<M: Into<String>>(message: M) -> Self {
            Self {
                message: message.into(),
                code: None,
                var_name: None,
                retry_after: None,
            }
        }

        pub fn bad_request() -> Self {
            let mut res = Self::new("Bad request");
            res.code = Some("BAD_REQUEST".to_owned());
            res
        }

        pub fn request_var_missing() -> Self {
            let mut res = Self::new("Var is missing");
            res.code = Some("REQUEST_VARIABLE_MISSING".to_owned());
            res.var_name = Some("Foo".to_owned());
            res
        }

        pub fn rate_limit() -> Self {
            let mut res = Self::new("API usage exceeded rate limit");
            res.code = Some("RATE_LIMIT_HIT".to_owned());
            res.retry_after = Some(28.706807374954224);
            res
        }

        pub fn user_deactivated() -> Self {
            let mut res = Self::new("User deactivated");
            res.code = Some("USER_DEACTIVATED".to_owned());
            res
        }

        pub fn realm_deactivated() -> Self {
            let mut res = Self::new("User deactivated");
            res.code = Some("REALM_DEACTIVATED".to_owned());
            res
        }

        pub fn auth_failed() -> Self {
            let mut res = Self::new("Your username or password is incorrect");
            res.code = Some("AUTHENTICATION_FAILED".to_owned());
            res
        }
    }


pub fn mock(response: ResponseTemplate, endpoint: &str) -> Mock {
    Mock::given(matchers::method("POST"))
        .and(matchers::path(format!(
            "{}{}",
            Endpoint::BASE_API,
            endpoint
        )))
        .respond_with(response)
        .expect(1)
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
