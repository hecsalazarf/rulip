use reqwest::Error as HttpError;
use serde::Deserialize;
use std::error::Error as StdError;
use std::fmt;

pub struct Error {
    kind: ErrorKind,
    source: Option<Box<dyn StdError>>,
}

impl Error {
    fn new_builder(http_error: HttpError) -> Self {
        Self {
            kind: ErrorKind::Build,
            source: Some(Box::new(http_error)),
        }
    }

    fn new_http(http_error: HttpError) -> Self {
        Self {
            kind: ErrorKind::Http(http_error),
            source: None,
        }
    }

    pub(crate) fn new_zulip(zulip_error: ZulipError) -> Self {
        Self {
            kind: ErrorKind::Zulip(zulip_error),
            source: None,
        }
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn is_zulip(&self) -> bool {
        match self.kind {
            ErrorKind::Zulip(_) => true,
            _ => false,
        }
    }

    pub fn is_http(&self) -> bool {
        match self.kind {
            ErrorKind::Http(_) => true,
            _ => false,
        }
    }

    pub fn is_build(&self) -> bool {
        match self.kind {
            ErrorKind::Build => true,
            _ => false,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self.kind {
            ErrorKind::Build => {
                f.write_str("builder error")?;
                if let Some(ref source) = self.source {
                    write!(f, ": {}", source)?;
                }
            }
            ErrorKind::Zulip(ref zulip) => write!(f, "zulip error: {}", zulip.message)?,
            ErrorKind::Http(ref http_e) => write!(f, "http client error: {}", http_e)?,
        }

        Ok(())
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut builder = f.debug_struct("rulip::Error");

        builder.field("kind", &self.kind);

        if let Some(ref source) = self.source() {
            builder.field("source", source);
        }

        builder.finish()
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source.as_ref().map(|s| s.as_ref())
    }
}

impl From<HttpError> for Error {
    fn from(http_error: HttpError) -> Self {
        if http_error.is_builder() {
            Error::new_builder(http_error)
        } else {
            Error::new_http(http_error)
        }
    }
}

#[derive(Debug)]
pub enum ErrorKind {
    Zulip(ZulipError),
    Build,
    Http(HttpError),
}

#[derive(Deserialize, Debug)]
pub struct ZulipError {
    message: String,
    #[serde(flatten)]
    code: Option<ZulipErrorCode>,
}

impl ZulipError {
    pub fn code(&self) -> Option<&ZulipErrorCode> {
        self.code.as_ref()
    }

    pub fn is_bad_request(&self) -> bool {
        match self.code {
            Some(ZulipErrorCode::BadRequest) => true,
            _ => false,
        }
    }

    pub fn is_rate_limit_hit(&self) -> bool {
        match self.code {
            Some(ZulipErrorCode::RateLimitHit { retry_after: _ }) => true,
            _ => false,
        }
    }

    pub fn is_realm_deactivated(&self) -> bool {
        match self.code {
            Some(ZulipErrorCode::RealmDeactivated) => true,
            _ => false,
        }
    }

    pub fn is_user_deactivated(&self) -> bool {
        match self.code {
            Some(ZulipErrorCode::UserDeactivated) => true,
            _ => false,
        }
    }

    pub fn is_variable_missing(&self) -> bool {
        match self.code {
            Some(ZulipErrorCode::RequestVariableMissing { var_name: _ }) => true,
            _ => false,
        }
    }

    pub fn is_auth_failed(&self) -> bool {
        match self.code {
            Some(ZulipErrorCode::AuthenticationFailed) => true,
            _ => false,
        }
    }
}

impl fmt::Display for ZulipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        if let Some(ref code) = self.code {
            match code {
                ZulipErrorCode::BadRequest => write!(f, "bad request: {}", self.message)?,
                ZulipErrorCode::RateLimitHit { retry_after } => {
                    write!(f, "rate limit hit, retry after {}s", retry_after)?
                }
                ZulipErrorCode::RealmDeactivated => {
                    write!(f, "realm deactivated: {}", self.message)?
                }
                ZulipErrorCode::UserDeactivated => {
                    write!(f, "account deativated: {}", self.message)?
                }
                ZulipErrorCode::RequestVariableMissing { var_name } => {
                    write!(f, "missing '{}' argument", var_name)?
                }
                ZulipErrorCode::AuthenticationFailed => {
                    write!(f, "authentication failed: {}", self.message)?
                }
            }
        } else {
            f.write_str(self.message.as_str())?;
        }

        Ok(())
    }
}

impl StdError for ZulipError {}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "code")]
#[non_exhaustive]
pub enum ZulipErrorCode {
    #[serde(rename = "BAD_REQUEST")]
    BadRequest,
    #[serde(rename = "REQUEST_VARIABLE_MISSING")]
    RequestVariableMissing { var_name: String },
    #[serde(rename = "USER_DEACTIVATED")]
    UserDeactivated,
    #[serde(rename = "REALM_DEACTIVATED")]
    RealmDeactivated,
    #[serde(rename = "RATE_LIMIT_HIT")]
    RateLimitHit { retry_after: f32 },
    #[serde(rename = "AUTHENTICATION_FAILED")]
    AuthenticationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{mock, MockErrorResponse};
    use reqwest::Client as HttpClient;
    use wiremock::{MockServer, ResponseTemplate};

    async fn send_request(
        httpc: &HttpClient,
        uri: String,
        endpoint: &str,
    ) -> Result<ZulipError, reqwest::Error> {
        httpc
            .post(format!("{}/api/v1/{}", uri, endpoint))
            .send()
            .await?
            .json()
            .await
    }

    #[tokio::test]
    async fn zulip_error_code() -> Result<(), Box<dyn std::error::Error>> {
        let server = MockServer::start().await;
        server
            .register(mock(
                ResponseTemplate::new(400).set_body_json(MockErrorResponse::rate_limit()),
                "rate_limit",
            ))
            .await;
        server
            .register(mock(
                ResponseTemplate::new(400).set_body_json(MockErrorResponse::bad_request()),
                "bad_request",
            ))
            .await;
        server
            .register(mock(
                ResponseTemplate::new(400).set_body_json(MockErrorResponse::request_var_missing()),
                "var_missing",
            ))
            .await;
        server
            .register(mock(
                ResponseTemplate::new(400)
                    .set_body_json(MockErrorResponse::new("Some description")),
                "no_code",
            ))
            .await;
        server
            .register(mock(
                ResponseTemplate::new(400).set_body_json(MockErrorResponse::user_deactivated()),
                "user_deactivated",
            ))
            .await;
        server
            .register(mock(
                ResponseTemplate::new(400).set_body_json(MockErrorResponse::realm_deactivated()),
                "realm_deactivated",
            ))
            .await;
        server
            .register(mock(
                ResponseTemplate::new(400).set_body_json(MockErrorResponse::auth_failed()),
                "auth_failed",
            ))
            .await;

        let httpc = HttpClient::new();
        let mut res = send_request(&httpc, server.uri(), "rate_limit").await?;
        assert!(res.is_rate_limit_hit());
        res = send_request(&httpc, server.uri(), "bad_request").await?;
        assert!(res.is_bad_request());
        res = send_request(&httpc, server.uri(), "var_missing").await?;
        assert!(res.is_variable_missing());
        res = send_request(&httpc, server.uri(), "realm_deactivated").await?;
        assert!(res.is_realm_deactivated());
        res = send_request(&httpc, server.uri(), "user_deactivated").await?;
        assert!(res.is_user_deactivated());
        res = send_request(&httpc, server.uri(), "auth_failed").await?;
        assert!(res.is_auth_failed());
        res = send_request(&httpc, server.uri(), "no_code").await?;
        assert_eq!(res.code, None);
        Ok(())
    }
}
