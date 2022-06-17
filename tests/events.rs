use dotenv::dotenv;
use rulip::{Client, Error};
use std::env;
use std::sync::Once;
static INIT: Once = Once::new();

pub fn initialize() {
    INIT.call_once(|| {
        dotenv().ok();
    });
}

#[tokio::test]
#[ignore]
async fn register_unregister() -> Result<(), Error> {
    initialize();
    let username = env::var("ZULIP_USERNAME").expect("Zulip username");
    let api_key = env::var("ZULIP_API_KEY").expect("Zulip API key");
    let uri = env::var("ZULIP_URI").expect("Zulip URI");

    let client = Client::build(uri)
        .with_key(username, api_key)
        .init()
        .await?;
    let queue = client.queue().register().await?;
    println!("Queue registered with ID: '{}'", queue.id());
    assert!(queue.id().len() > 0);
    assert_eq!(queue.last_event_id(), -1);
    queue.unregister().await?;
    Ok(())
}
