# MQTT Client API Reference

Complete API documentation for the MQTT v5.0 client library.

## Core Types

### MqttClient

The main client struct for MQTT operations.

```rust
pub struct MqttClient {
    // Internal implementation
}
```

#### Methods

##### new
```rust
pub fn new(client_id: impl Into<String>) -> Self
```

Creates a new MQTT client with the specified client ID.

**Parameters:**
- `client_id` - Unique identifier for this client

**Example:**
```rust
let client = MqttClient::new("device-001");
```

##### with_options
```rust
pub fn with_options(options: ConnectOptions) -> Self
```

Creates a new MQTT client with custom connection options.

**Parameters:**
- `options` - Connection configuration

**Example:**
```rust
let options = ConnectOptions::new("device-001");
let client = MqttClient::with_options(options);
```

##### connect
```rust
pub async fn connect(&self, address: &str) -> Result<()>
```

Connects to an MQTT broker at the specified address.

**Parameters:**
- `address` - Broker address in format `protocol://host:port`

**Supported protocols:**
- `mqtt://` - Plain TCP
- `mqtts://` - TLS/SSL
- `ws://` - WebSocket
- `wss://` - Secure WebSocket

**Example:**
```rust
client.connect("mqtt://broker.example.com:1883").await?;
```

##### disconnect
```rust
pub async fn disconnect(&self) -> Result<()>
```

Gracefully disconnects from the broker.

##### publish
```rust
pub async fn publish(&self, topic: &str, payload: &[u8]) -> Result<()>
```

Publishes a message with QoS 0 (fire and forget).

**Parameters:**
- `topic` - Topic to publish to
- `payload` - Message payload

##### publish_qos1
```rust
pub async fn publish_qos1(&self, topic: &str, payload: &[u8]) -> Result<u16>
```

Publishes a message with QoS 1 (at least once delivery).

**Returns:** Packet identifier for tracking

##### publish_qos2
```rust
pub async fn publish_qos2(&self, topic: &str, payload: &[u8]) -> Result<u16>
```

Publishes a message with QoS 2 (exactly once delivery).

**Returns:** Packet identifier for tracking

##### publish_with_options
```rust
pub async fn publish_with_options(
    &self, 
    topic: &str, 
    payload: &[u8], 
    options: PublishOptions
) -> Result<Option<u16>>
```

Publishes a message with custom options.

**Returns:** Packet identifier if QoS > 0

##### subscribe
```rust
pub async fn subscribe<F>(&self, topic_filter: &str, callback: F) -> Result<()>
where
    F: Fn(Message) + Send + Sync + 'static
```

Subscribes to a topic with a callback for received messages.

**Parameters:**
- `topic_filter` - Topic filter (supports wildcards)
- `callback` - Function called for each matching message

**Example:**
```rust
client.subscribe("sensors/+/temperature", |msg| {
    println!("Temperature from {}: {}", msg.topic, 
        String::from_utf8_lossy(&msg.payload));
}).await?;
```

##### subscribe_with_options
```rust
pub async fn subscribe_with_options<F>(
    &self, 
    topic_filter: &str, 
    options: SubscribeOptions,
    callback: F
) -> Result<()>
where
    F: Fn(Message) + Send + Sync + 'static
```

Subscribes with custom options.

##### unsubscribe
```rust
pub async fn unsubscribe(&self, topic_filter: &str) -> Result<()>
```

Unsubscribes from a topic.

##### on_connection_event
```rust
pub async fn on_connection_event<F>(&self, callback: F) -> Result<()>
where
    F: Fn(ConnectionEvent) + Send + Sync + 'static
```

Registers a callback for connection state changes.

### ConnectOptions

Configuration for client connections.

```rust
pub struct ConnectOptions {
    pub client_id: String,
    pub clean_start: bool,
    pub keep_alive: Duration,
    pub session_expiry_interval: Option<Duration>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub will: Option<LastWill>,
    pub reconnect_config: ReconnectConfig,
    // ... other fields
}
```

#### Methods

##### new
```rust
pub fn new(client_id: impl Into<String>) -> Self
```

Creates options with default values.

##### Default Values
- `clean_start`: true
- `keep_alive`: 60 seconds
- `session_expiry_interval`: None (session ends on disconnect)
- `reconnect_config`: Enabled with exponential backoff

### Message

Represents a received MQTT message.

```rust
pub struct Message {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: QoS,
    pub retain: bool,
    pub properties: MessageProperties,
}
```

### PublishOptions

Options for publishing messages.

```rust
pub struct PublishOptions {
    pub qos: QoS,
    pub retain: bool,
    pub duplicate: bool,
    pub properties: PublishProperties,
}
```

#### Default Implementation
```rust
impl Default for PublishOptions {
    fn default() -> Self {
        Self {
            qos: QoS::AtMostOnce,
            retain: false,
            duplicate: false,
            properties: PublishProperties::default(),
        }
    }
}
```

### SubscribeOptions

Options for subscriptions.

```rust
pub struct SubscribeOptions {
    pub qos: QoS,
    pub no_local: bool,
    pub retain_as_published: bool,
    pub retain_handling: RetainHandling,
}
```

### LastWill

Last Will and Testament configuration.

```rust
pub struct LastWill {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: QoS,
    pub retain: bool,
    pub properties: WillProperties,
}
```

**Example:**
```rust
let will = LastWill {
    topic: "devices/device-001/status".to_string(),
    payload: b"offline".to_vec(),
    qos: QoS::AtLeastOnce,
    retain: true,
    properties: WillProperties::default(),
};
```

### ReconnectConfig

Automatic reconnection configuration.

```rust
pub struct ReconnectConfig {
    pub enabled: bool,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub exponential_backoff: bool,
    pub max_attempts: Option<u32>,
}
```

**Default Values:**
- `enabled`: true
- `initial_delay`: 1 second
- `max_delay`: 120 seconds
- `exponential_backoff`: true
- `max_attempts`: None (unlimited)

## Enumerations

### QoS

Quality of Service levels.

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QoS {
    AtMostOnce = 0,   // Fire and forget
    AtLeastOnce = 1,  // Acknowledged delivery
    ExactlyOnce = 2,  // Assured delivery
}
```

### ConnectionEvent

Connection state events.

```rust
#[derive(Debug)]
pub enum ConnectionEvent {
    Connected { 
        session_present: bool 
    },
    Disconnected { 
        reason: DisconnectReason 
    },
    Reconnecting { 
        attempt: u32 
    },
    ReconnectFailed { 
        error: MqttError 
    },
}
```

### DisconnectReason

Reasons for disconnection.

```rust
#[derive(Debug)]
pub enum DisconnectReason {
    ClientInitiated,
    ServerInitiated(ReasonCode),
    NetworkError(std::io::Error),
    ProtocolError(String),
    KeepAliveTimeout,
}
```

### RetainHandling

How retained messages are handled on subscription.

```rust
#[derive(Debug, Clone, Copy)]
pub enum RetainHandling {
    SendAtSubscribe = 0,
    SendAtSubscribeIfNew = 1,
    DoNotSend = 2,
}
```

## Error Types

### MqttError

Main error type for MQTT operations.

```rust
#[derive(Debug, thiserror::Error)]
pub enum MqttError {
    #[error("Network error: {0}")]
    Network(#[from] std::io::Error),
    
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    #[error("Connection rejected: {0:?}")]
    ConnectionRejected(ConnectReasonCode),
    
    #[error("Not connected")]
    NotConnected,
    
    #[error("Operation timed out")]
    Timeout,
    
    #[error("Invalid topic: {0}")]
    InvalidTopic(String),
    
    // ... other variants
}
```

### ConnectReasonCode

MQTT v5.0 connection reason codes.

```rust
#[derive(Debug)]
pub enum ConnectReasonCode {
    Success = 0x00,
    UnspecifiedError = 0x80,
    MalformedPacket = 0x81,
    ProtocolError = 0x82,
    ImplementationSpecificError = 0x83,
    UnsupportedProtocolVersion = 0x84,
    ClientIdentifierNotValid = 0x85,
    BadUserNameOrPassword = 0x86,
    NotAuthorized = 0x87,
    ServerUnavailable = 0x88,
    ServerBusy = 0x89,
    Banned = 0x8A,
    BadAuthenticationMethod = 0x8C,
    TopicNameInvalid = 0x90,
    PacketTooLarge = 0x95,
    QuotaExceeded = 0x97,
    PayloadFormatInvalid = 0x99,
    RetainNotSupported = 0x9A,
    QoSNotSupported = 0x9B,
    UseAnotherServer = 0x9C,
    ServerMoved = 0x9D,
    ConnectionRateExceeded = 0x9F,
}
```

## Properties

### PublishProperties

MQTT v5.0 publish properties.

```rust
pub struct PublishProperties {
    pub payload_format_indicator: Option<PayloadFormatIndicator>,
    pub message_expiry_interval: Option<u32>,
    pub topic_alias: Option<u16>,
    pub response_topic: Option<String>,
    pub correlation_data: Option<Vec<u8>>,
    pub user_properties: Vec<(String, String)>,
    pub subscription_identifier: Option<NonZeroU32>,
    pub content_type: Option<String>,
}
```

### MessageProperties

Properties of received messages.

```rust
pub struct MessageProperties {
    pub payload_format_indicator: Option<PayloadFormatIndicator>,
    pub message_expiry_interval: Option<u32>,
    pub topic_alias: Option<u16>,
    pub response_topic: Option<String>,
    pub correlation_data: Option<Vec<u8>>,
    pub user_properties: Vec<(String, String)>,
    pub subscription_identifiers: Vec<NonZeroU32>,
    pub content_type: Option<String>,
}
```

### WillProperties

Last Will properties.

```rust
pub struct WillProperties {
    pub will_delay_interval: Option<u32>,
    pub payload_format_indicator: Option<PayloadFormatIndicator>,
    pub message_expiry_interval: Option<u32>,
    pub content_type: Option<String>,
    pub response_topic: Option<String>,
    pub correlation_data: Option<Vec<u8>>,
    pub user_properties: Vec<(String, String)>,
}
```

## Utility Types

### PayloadFormatIndicator

Indicates payload format.

```rust
#[derive(Debug, Clone, Copy)]
pub enum PayloadFormatIndicator {
    Unspecified = 0,
    Utf8 = 1,
}
```

## Advanced Usage

### Custom Transport

Implement custom transports by implementing the `Transport` trait:

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    async fn read_packet(&mut self) -> Result<Packet>;
    async fn write_packet(&mut self, packet: Packet) -> Result<()>;
    fn is_connected(&self) -> bool;
}
```

### Packet Types

Low-level MQTT packet types (for advanced usage):

```rust
pub enum Packet {
    Connect(ConnectPacket),
    ConnAck(ConnAckPacket),
    Publish(PublishPacket),
    PubAck(PubAckPacket),
    PubRec(PubRecPacket),
    PubRel(PubRelPacket),
    PubComp(PubCompPacket),
    Subscribe(SubscribePacket),
    SubAck(SubAckPacket),
    Unsubscribe(UnsubscribePacket),
    UnsubAck(UnsubAckPacket),
    PingReq,
    PingResp,
    Disconnect(DisconnectPacket),
    Auth(AuthPacket),
}
```

## Thread Safety

All public methods on `MqttClient` are thread-safe and can be called from multiple threads concurrently. The client uses internal synchronization to ensure safe access to shared state.

## Performance Considerations

1. **Message Callbacks**: Keep callbacks lightweight to avoid blocking the packet reader
2. **Large Payloads**: Consider streaming for payloads > 1MB
3. **Topic Filters**: More specific filters perform better than broad wildcards
4. **Connection Pooling**: Reuse client instances rather than creating new ones

## Examples

### Basic Pub/Sub
```rust
use mqtt5::{MqttClient, QoS};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = MqttClient::new("example-client");
    client.connect("mqtt://test.mosquitto.org:1883").await?;
    
    // Subscribe
    client.subscribe("test/topic", |msg| {
        println!("Received: {}", String::from_utf8_lossy(&msg.payload));
    }).await?;
    
    // Publish
    client.publish_qos1("test/topic", b"Hello MQTT").await?;
    
    tokio::time::sleep(Duration::from_secs(1)).await;
    client.disconnect().await?;
    
    Ok(())
}
```

### With Connection Events
```rust
use mqtt5::{MqttClient, ConnectionEvent};

let client = MqttClient::new("monitored-client");

client.on_connection_event(|event| {
    match event {
        ConnectionEvent::Connected { session_present } => {
            println!("Connected! Session present: {}", session_present);
        }
        ConnectionEvent::Disconnected { reason } => {
            println!("Disconnected: {:?}", reason);
        }
        _ => {}
    }
}).await?;
```

### Error Handling
```rust
use mqtt5::{MqttClient, MqttError};

match client.connect("mqtt://broker:1883").await {
    Ok(_) => println!("Connected"),
    Err(MqttError::Network(e)) => eprintln!("Network error: {}", e),
    Err(MqttError::ConnectionRejected(code)) => {
        eprintln!("Connection rejected: {:?}", code);
    }
    Err(e) => eprintln!("Error: {}", e),
}
```