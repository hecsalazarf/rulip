mod endpoint;

#[cfg(test)]
mod test_util;

use endpoint::Endpoint;
use reqwest::Client as HttpClient;
use reqwest::{IntoUrl, Method, Url};
use serde::{Deserialize, Serialize};

pub struct Client {
    base_uri: Url,
    http: HttpClient,
    credentials: Option<Credentials>,
}

impl Client {
    pub fn build<U: IntoUrl>(uri: U) -> ClientBuilder {
        ClientBuilder::new(uri.into_url())
    }

    fn set_credentials(&mut self, credentials: Credentials) {
        self.credentials.replace(credentials);
    }

    async fn send_request<T, R>(
        &self,
        method: Method,
        endpoint: &str,
        params: T,
    ) -> Result<R, Box<dyn std::error::Error>>
    where
        T: Serialize,
        R: serde::de::DeserializeOwned,
    {
        let mut req = self
            .http
            .request(method, self.base_uri.join(endpoint).unwrap())
            .form(&params);

        if let Some(ref credentials) = self.credentials {
            req = req.basic_auth(credentials.username(), credentials.password());
        }

        let res = req.send().await?.json().await?;

        Ok(res)
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

    pub async fn init(self) -> Result<Client, Box<dyn std::error::Error>> {
        let mut client = Client {
            credentials: None,
            http: reqwest::Client::new(),
            // Notice the slash at the beginning and at the end in order to replace any path
            // from the URI. We append the API path to the domain.
            base_uri: self.uri?.join(Endpoint::BASE_API).unwrap(),
        };

        if let Some(key) = self.api_key {
            client.set_credentials(Credentials::new(self.user.unwrap(), key));
        } else if let Some(password) = self.password {
            // Fetch API key from production server, providing username and password.
            let params = Credentials::new(self.user.unwrap(), password);

            let res = client
                .send_request(Method::POST, Endpoint::FETCH_API_KEY, params)
                .await?;

            client.set_credentials(res);
        } else if let Some(user) = self.user {
            // Fetch API key from dev server, providing username only.
            let params = Credentials::unauthenticated(user);
            let res = client
                .send_request(Method::POST, Endpoint::FETCH_DEV_API_KEY, params)
                .await?;

            client.set_credentials(res);
        }

        Ok(client)
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
    use test_util::{body_as_string, mock_server, MockAuthResponse, MockCredentials};
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

        let credentials = client.credentials.as_ref().unwrap();
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

        let credentials = client.credentials.as_ref().unwrap();
        // Check credentials
        assert_eq!(credentials.username(), MockCredentials::USERNAME);
        assert_eq!(credentials.password(), Some(MockCredentials::API_KEY));
        Ok(())
    }

    #[tokio::test]
    async fn unauthenticated() -> Result<(), Box<dyn std::error::Error>> {
        let client = Client::build("https://hello.zulipchat.com").init().await?;

        // Check credentials
        assert_eq!(client.credentials, None);
        Ok(())
    }

    #[tokio::test]
    async fn base_uri() {
        const CANONICAL_URI: &str = "https://hello.zulipchat.com";
        const BASE_URI: &str = "https://hello.zulipchat.com/api/v1/";
        let mut res = Client::build(CANONICAL_URI).init().await;
        assert_eq!(
            res.unwrap().base_uri.as_str(),
            BASE_URI,
            "Expect the base URI of API"
        );

        res = Client::build(CANONICAL_URI.to_owned() + "/diff/path")
            .init()
            .await;
        assert_eq!(
            res.unwrap().base_uri.as_str(),
            BASE_URI,
            "Expect removal of existing path"
        );

        res = Client::build("invalid_uri").init().await;
        assert!(res.is_err(), "Expect invalid URI");
    }
}
