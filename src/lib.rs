mod endpoint;
mod error;
pub mod event;

#[cfg(test)]
mod test_util;

pub use error::Error;

use endpoint::Endpoint;
use event::QueueBuilder;
use reqwest::Client as HttpClient;
use reqwest::{IntoUrl, Method, Response, Url};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

impl Client {
    fn new(inner: ClientInner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub(crate) async fn send<T, R>(
        &self,
        method: Method,
        endpoint: &str,
        params: &T,
    ) -> Result<R, Error>
    where
        T: Serialize,
        R: serde::de::DeserializeOwned,
    {
        self.inner.send(method, endpoint, params).await
    }

    pub async fn send_request<S, T>(
        &self,
        method: Method,
        endpoint: S,
        params: &T,
    ) -> reqwest::Result<reqwest::Response>
    where
        S: AsRef<str>,
        T: Serialize,
    {
        self.inner.send_request(method, endpoint, params).await
    }

    pub fn build<U: IntoUrl>(uri: U) -> ClientBuilder {
        ClientBuilder::new(uri.into_url())
    }

    pub fn queue(&self) -> QueueBuilder {
        QueueBuilder::new(self.clone())
    }
}

#[derive(Debug)]
pub struct ClientInner {
    base_uri: Url,
    http: HttpClient,
    credentials: Option<Credentials>,
}

impl ClientInner {
    fn new(base_uri: Url) -> Self {
        Self {
            credentials: None,
            http: reqwest::Client::new(),
            base_uri,
        }
    }

    fn set_credentials(&mut self, credentials: Credentials) {
        self.credentials.replace(credentials);
    }

    async fn deserialize<R>(response: Response) -> R
    where
        R: serde::de::DeserializeOwned,
    {
        match response.json().await {
            Ok(data) => data,
            // Since this function is internally called, we should know the type
            // of data we expect, otherwise there is implementation issue
            Err(e) => panic!("{}. This should not happen, report the issue", e),
        }
    }

    async fn send<T, R>(&self, method: Method, endpoint: &str, params: &T) -> Result<R, Error>
    where
        T: Serialize,
        R: serde::de::DeserializeOwned,
    {
        let res = self.send_request(method, endpoint, params).await?;

        if res.status().is_client_error() {
            // Create error from body
            Err(Error::new_zulip(Self::deserialize(res).await))
        } else if res.status().is_server_error() {
            // Create error from status
            res.error_for_status()?;
            // Unreachable because we already know that the response has status error code
            unreachable!();
        } else if res.status().is_informational() || res.status().is_redirection() {
            unimplemented!();
        } else {
            // Successful response
            Ok(Self::deserialize(res).await)
        }
    }

    pub async fn send_request<S, T>(
        &self,
        method: Method,
        endpoint: S,
        params: &T,
    ) -> reqwest::Result<reqwest::Response>
    where
        S: AsRef<str>,
        T: Serialize,
    {
        let mut req = self.http.request(
            method.clone(),
            self.base_uri.join(endpoint.as_ref()).unwrap(),
        );

        if method == Method::GET {
            req = req.query(params)
        } else {
            req = req.form(params)
        }

        if let Some(ref credentials) = self.credentials {
            req = req.basic_auth(credentials.username(), credentials.password());
        }

        req.send().await
    }

    pub fn build<U: IntoUrl>(uri: U) -> ClientBuilder {
        ClientBuilder::new(uri.into_url())
    }
}

pub struct ClientBuilder {
    uri: reqwest::Result<Url>,
    user: Option<String>,
    password: Option<String>,
    api_key: Option<String>,
}

impl ClientBuilder {
    fn new(uri: reqwest::Result<Url>) -> Self {
        Self {
            uri,
            user: None,
            password: None,
            api_key: None,
        }
    }

    pub fn with_credentials<U, T>(mut self, user: U, password: Option<T>) -> Self
    where
        U: Into<String>,
        T: Into<String>,
    {
        self.password = password.map(|p| p.into());
        self.user.replace(user.into());
        self
    }

    pub fn with_key<U, K>(mut self, user: U, key: K) -> Self
    where
        U: Into<String>,
        K: Into<String>,
    {
        self.api_key.replace(key.into());
        self.user.replace(user.into());
        self
    }

    pub async fn init(self) -> Result<Client, Error> {
        // Notice the slash at the beginning and at the end in order to replace any path
        // from the URI. We append the API path to the domain.
        let base_uri = self.uri?.join(Endpoint::BASE_API).unwrap();
        let mut inner = ClientInner::new(base_uri);

        if let Some(key) = self.api_key {
            inner.set_credentials(Credentials::new(self.user.unwrap(), key));
        } else if let Some(password) = self.password {
            // Fetch API key from production server, providing username and password.
            let params = Credentials::new(self.user.unwrap(), password);

            let res = inner
                .send(Method::POST, Endpoint::FETCH_API_KEY, &params)
                .await?;

            inner.set_credentials(res);
        } else if let Some(user) = self.user {
            // Fetch API key from dev server, providing username only.
            let params = Credentials::unauthenticated(user);
            let res = inner
                .send(Method::POST, Endpoint::FETCH_DEV_API_KEY, &params)
                .await?;

            inner.set_credentials(res);
        }

        Ok(Client::new(inner))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Credentials {
    #[serde(rename(deserialize = "email"))]
    username: String,
    #[serde(rename(deserialize = "api_key"))]
    password: Option<String>,
}

impl Credentials {
    fn new(username: String, password: String) -> Self {
        Self {
            username,
            password: Some(password),
        }
    }

    fn unauthenticated(username: String) -> Self {
        Self {
            username,
            password: None,
        }
    }

    fn username(&self) -> &str {
        self.username.as_str()
    }

    fn password(&self) -> Option<&str> {
        self.password.as_ref().map(|p| p.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use test_util::{
        body_as_string, mock_server, MockAuthResponse, MockCredentials, MockErrorResponse,
    };
    use wiremock::ResponseTemplate;

    fn auth_response() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(MockAuthResponse::new())
    }

    #[tokio::test]
    async fn prod_auth() -> Result<(), Box<dyn std::error::Error>> {
        let server = mock_server(auth_response(), Endpoint::FETCH_API_KEY).await;
        let client = Client::build(server.uri())
            .with_credentials(MockCredentials::USERNAME, Some(MockCredentials::PASSWORD))
            .init()
            .await?;

        // Check username and password were sent to the server
        assert_eq!(
            body_as_string(&server).await?.unwrap(),
            format!(
                "username={}&password={}",
                MockCredentials::USERNAME,
                MockCredentials::PASSWORD
            )
        );

        let credentials = client.inner.credentials.as_ref().unwrap();
        // Check credentials
        assert_eq!(credentials.username(), MockCredentials::USERNAME);
        assert_eq!(credentials.password(), Some(MockCredentials::API_KEY));
        Ok(())
    }

    #[tokio::test]
    async fn dev_auth() -> Result<(), Box<dyn std::error::Error>> {
        let server = mock_server(auth_response(), Endpoint::FETCH_DEV_API_KEY).await;
        let client = Client::build(server.uri())
            .with_credentials(MockCredentials::USERNAME, None::<String>)
            .init()
            .await?;

        // Check that only username was sent to the server
        assert_eq!(
            body_as_string(&server).await?.unwrap(),
            format!("username={}", MockCredentials::USERNAME)
        );

        let credentials = client.inner.credentials.as_ref().unwrap();
        // Check credentials
        assert_eq!(credentials.username(), MockCredentials::USERNAME);
        assert_eq!(credentials.password(), Some(MockCredentials::API_KEY));
        Ok(())
    }

    #[tokio::test]
    async fn unauthenticated() -> Result<(), Error> {
        let client = Client::build("https://hello.zulipchat.com").init().await?;

        // Check credentials
        assert_eq!(client.inner.credentials, None);
        Ok(())
    }

    #[tokio::test]
    async fn auth_fail() -> Result<(), Box<dyn std::error::Error>> {
        let template = ResponseTemplate::new(401).set_body_json(MockErrorResponse::auth_failed());
        let server = mock_server(template, Endpoint::FETCH_API_KEY).await;
        let error = Client::build(server.uri())
            .with_credentials(MockCredentials::USERNAME, Some(MockCredentials::PASSWORD))
            .init()
            .await
            .expect_err("Client initialization should return an error");

        match error.kind() {
            ErrorKind::Zulip(e) => assert!(e.is_auth_failed()),
            _ => unreachable!(),
        }
        Ok(())
    }

    #[tokio::test]
    async fn base_uri() -> Result<(), Error> {
        const CANONICAL_URI: &str = "https://hello.zulipchat.com";
        const BASE_URI: &str = "https://hello.zulipchat.com/api/v1/";
        let mut client = Client::build(CANONICAL_URI).init().await?;
        assert_eq!(
            client.inner.base_uri.as_str(),
            BASE_URI,
            "Expect the base URI of API"
        );

        client = Client::build(CANONICAL_URI.to_owned() + "/diff/path")
            .init()
            .await?;
        assert_eq!(
            client.inner.base_uri.as_str(),
            BASE_URI,
            "Expect removal of existing path"
        );

        let error = Client::build("invalid_uri").init().await.err().unwrap();
        assert!(error.is_build(), "Expect invalid URI");
        Ok(())
    }
}
