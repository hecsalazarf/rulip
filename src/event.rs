use crate::{Client, Endpoint, Error};
use reqwest::Method;
use serde::{Deserialize, Serialize};

pub struct Queue {
    dispatcher: Dispatcher,
}

impl Queue {
    fn new(client: Client, response: RegisterQueueResponse) -> Self {
        let dispatcher = Dispatcher {
            params: DispatcherParams {
                queue_id: response.queue_id,
                last_event_id: response.last_event_id,
            },
            client,
        };

        Queue { dispatcher }
    }

    pub fn id(&self) -> &str {
        self.dispatcher.queue_id()
    }

    pub fn last_event_id(&self) -> i32 {
        self.dispatcher.last_event_id()
    }

    pub async fn events(&mut self) -> Result<Vec<Event>, Error> {
        self.dispatcher.events().await
    }

    pub async fn unregister(self) -> Result<(), Error> {
        self.dispatcher.unregister().await?;
        Ok(())
    }
}

pub struct QueueBuilder {
    request: RegisterQueueRequest,
    client: Client,
}

impl QueueBuilder {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            request: RegisterQueueRequest::default(),
            client,
        }
    }

    pub fn apply_markdown(mut self, value: bool) -> Self {
        self.request.apply_markdown.replace(value);
        self
    }

    pub fn client_gravatar(mut self, value: bool) -> Self {
        self.request.client_gravatar.replace(value);
        self
    }

    pub fn slim_presence(mut self, value: bool) -> Self {
        self.request.slim_presence.replace(value);
        self
    }

    pub fn all_public_streams(mut self, value: bool) -> Self {
        self.request.all_public_streams.replace(value);
        self
    }

    pub fn include_subscribers(mut self, value: bool) -> Self {
        self.request.include_subscribers.replace(value);
        self
    }

    pub fn for_event<T: Into<String>>(mut self, event: T) -> Self {
        let event = event.into();
        let events = self
            .request
            .event_types
            .get_or_insert(Vec::with_capacity(1));

        if events.iter().find(|evt| evt == &&event).is_none() {
            events.push(event);
        }
        self
    }

    pub fn narrow<C, V>(mut self, condition: C, value: V) -> Self
    where
        C: Into<String>,
        V: Into<String>,
    {
        let condition = condition.into();
        let value = value.into();

        let events = self.request.narrow.get_or_insert(Vec::with_capacity(1));
        if events
            .iter()
            .find(|[c, v]| c == &condition && v == &value)
            .is_none()
        {
            events.push([condition, value]);
        }
        self
    }

    pub async fn register(self) -> Result<Queue, Error> {
        let response: RegisterQueueResponse = self
            .client
            .send(Method::POST, Endpoint::REGISTER_EVENT_QUEUE, &self.request)
            .await?;

        Ok(Queue::new(self.client, response))
    }
}

#[derive(Clone)]
struct Dispatcher {
    params: DispatcherParams,
    client: Client,
}

impl Dispatcher {
    fn queue_id(&self) -> &str {
        self.params.queue_id.as_str()
    }

    fn last_event_id(&self) -> i32 {
        self.params.last_event_id
    }

    async fn fetch_events(&self) -> Result<EventsResponse, Error> {
        self.client
            .send(Method::GET, Endpoint::EVENTS_QUEUE, &self.params)
            .await
    }

    async fn events(&mut self) -> Result<Vec<Event>, Error> {
        loop {
            let events = self.fetch_events().await?.events;
            if let Some(evt) = events.iter().last() {
                self.params.last_event_id = evt.id();
                if evt.kind.as_str() == "heartbeat" {
                    continue;
                }
            }
            break Ok(events);
        }
    }

    async fn unregister(&self) -> Result<EmptyResponse, Error> {
        let params = UnregisterQueueRequest::from(&self.params);
        self.client
            .send(Method::DELETE, Endpoint::EVENTS_QUEUE, &params)
            .await
    }
}

#[derive(Serialize, Clone)]
struct DispatcherParams {
    queue_id: String,
    last_event_id: i32,
}

#[derive(Serialize)]
struct UnregisterQueueRequest<'a> {
    queue_id: &'a str,
}

impl<'a> From<&'a DispatcherParams> for UnregisterQueueRequest<'a> {
    fn from(value: &'a DispatcherParams) -> Self {
        Self {
            queue_id: &value.queue_id,
        }
    }
}

#[derive(Serialize, Default)]
struct RegisterQueueRequest {
    apply_markdown: Option<bool>,
    client_gravatar: Option<bool>,
    slim_presence: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(serialize_with = "crate::serialize")]
    event_types: Option<Vec<String>>,
    all_public_streams: Option<bool>,
    include_subscribers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(serialize_with = "crate::serialize")]
    narrow: Option<Vec<[String; 2]>>,
    #[serde(serialize_with = "crate::serialize")]
    client_capabilities: ClientCapabilities,
}

#[derive(Serialize)]
struct ClientCapabilities {
    notification_settings_null: Option<bool>,
    bulk_message_deletion: Option<bool>,
    user_avatar_url_field_optional: Option<bool>,
    stream_typing_notifications: Option<bool>,
    user_settings_object: Option<bool>,
}

impl Default for ClientCapabilities {
    fn default() -> Self {
        Self {
            notification_settings_null: Some(true),
            bulk_message_deletion: Some(true),
            user_avatar_url_field_optional: Some(true),
            stream_typing_notifications: Some(true),
            user_settings_object: Some(true),
        }
    }
}

#[derive(Deserialize)]
struct RegisterQueueResponse {
    queue_id: String,
    // zulip_version: String,
    // zulip_feature_level: u16,
    // zulip_merge_base: String,
    last_event_id: i32,
}

#[derive(Deserialize)]
struct EventsResponse {
    events: Vec<Event>,
}

#[derive(Deserialize, Debug)]
pub struct Event {
    id: i32,
    #[serde(rename = "type")]
    kind: String,
    op: Option<EventOp>,
}

impl Event {
    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn op(&self) -> Option<EventOp> {
        self.op
    }
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub enum EventOp {
    #[serde(rename = "update")]
    Update,
    #[serde(rename = "add")]
    Add,
    #[serde(rename = "remove")]
    Remove,
    #[serde(rename = "peer_add")]
    PeerAdd,
    #[serde(rename = "peer_remove")]
    PeerRemove,
    #[serde(rename = "create")]
    Create,
    #[serde(rename = "delete")]
    Delete,
    #[serde(rename = "start")]
    Start,
    #[serde(rename = "stop")]
    Stop,
    #[serde(rename = "add_members")]
    AddMembers,
    #[serde(rename = "remove_members")]
    RemoveMembers,
    #[serde(rename = "add_subgroups")]
    AddSubgroups,
    #[serde(rename = "remove_subgroups")]
    RemoveSubgroups,
    #[serde(rename = "change")]
    Change,
    #[serde(rename = "deactivated")]
    Deactivated,
    #[serde(rename = "update_dict")]
    UpdateDict,
}

#[derive(Deserialize)]
struct EmptyResponse {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn queue_builder() -> Result<(), Error> {
        let client = Client::build("https://example.com").init().await?;
        let builder = client
            .queue()
            .for_event("reaction")
            .for_event("reaction")
            .narrow("stream", "general")
            .narrow("is", "public")
            .narrow("is", "public")
            .narrow("is", "private");

        assert_eq!(
            builder.request.narrow,
            Some(vec![
                ["stream".into(), "general".into()],
                ["is".into(), "public".into()],
                ["is".into(), "private".into()],
            ])
        );
        assert_eq!(builder.request.event_types, Some(vec!["reaction".into()]));
        Ok(())
    }
}
