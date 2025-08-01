//! Storage layer for MQTT broker persistence
//! 
//! Provides durable storage for retained messages, client sessions, and message queues.
//! Designed for production use with atomic operations and efficient file-based storage.

pub mod file_backend;
pub mod memory_backend;
pub mod queue;
pub mod retained;
pub mod sessions;

pub use file_backend::FileBackend;
pub use memory_backend::MemoryBackend;
pub use queue::MessageQueue;
pub use retained::RetainedMessages;
pub use sessions::SessionManager;

#[cfg(test)]
mod tests;

use crate::error::Result;
use crate::packet::publish::PublishPacket;
use crate::QoS;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use std::sync::Arc;

/// Retained message with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetainedMessage {
    /// Topic name
    pub topic: String,
    /// Message payload
    pub payload: Vec<u8>,
    /// Quality of Service level
    pub qos: QoS,
    /// Retain flag
    pub retain: bool,
    /// When the message was stored
    pub stored_at: SystemTime,
    /// Message expiry time (if any)
    pub expires_at: Option<SystemTime>,
}

/// Client session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSession {
    /// Client identifier
    pub client_id: String,
    /// Whether the session should persist after disconnect
    pub persistent: bool,
    /// Session expiry interval in seconds
    pub expiry_interval: Option<u32>,
    /// Active subscriptions
    pub subscriptions: HashMap<String, QoS>,
    /// Session creation time
    pub created_at: SystemTime,
    /// Last activity time
    pub last_seen: SystemTime,
}

/// Queued message for offline client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedMessage {
    /// Topic name
    pub topic: String,
    /// Message payload
    pub payload: Vec<u8>,
    /// Target client ID
    pub client_id: String,
    /// QoS level for delivery
    pub qos: QoS,
    /// When the message was queued
    pub queued_at: SystemTime,
    /// Message expiry time (if any)
    pub expires_at: Option<SystemTime>,
    /// Packet ID for QoS 1/2 delivery
    pub packet_id: Option<u16>,
}

/// Storage backend trait for broker persistence
pub trait StorageBackend: Send + Sync {
    /// Store a retained message
    fn store_retained_message(&self, topic: &str, message: RetainedMessage) -> impl std::future::Future<Output = Result<()>> + Send;
    
    /// Retrieve a retained message
    fn get_retained_message(&self, topic: &str) -> impl std::future::Future<Output = Result<Option<RetainedMessage>>> + Send;
    
    /// Remove a retained message
    fn remove_retained_message(&self, topic: &str) -> impl std::future::Future<Output = Result<()>> + Send;
    
    /// Get all retained messages matching a topic filter
    fn get_retained_messages(&self, topic_filter: &str) -> impl std::future::Future<Output = Result<Vec<(String, RetainedMessage)>>> + Send;
    
    /// Store client session
    fn store_session(&self, session: ClientSession) -> impl std::future::Future<Output = Result<()>> + Send;
    
    /// Retrieve client session
    fn get_session(&self, client_id: &str) -> impl std::future::Future<Output = Result<Option<ClientSession>>> + Send;
    
    /// Remove client session
    fn remove_session(&self, client_id: &str) -> impl std::future::Future<Output = Result<()>> + Send;
    
    /// Queue message for offline client
    fn queue_message(&self, message: QueuedMessage) -> impl std::future::Future<Output = Result<()>> + Send;
    
    /// Get queued messages for client
    fn get_queued_messages(&self, client_id: &str) -> impl std::future::Future<Output = Result<Vec<QueuedMessage>>> + Send;
    
    /// Remove queued messages for client
    fn remove_queued_messages(&self, client_id: &str) -> impl std::future::Future<Output = Result<()>> + Send;
    
    /// Clean up expired messages and sessions
    fn cleanup_expired(&self) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// In-memory storage for fast access with persistent backing
#[derive(Debug)]
pub struct Storage<B: StorageBackend> {
    /// Persistent storage backend
    backend: Arc<B>,
    /// In-memory retained messages cache
    retained_cache: Arc<RwLock<HashMap<String, RetainedMessage>>>,
    /// In-memory sessions cache
    sessions_cache: Arc<RwLock<HashMap<String, ClientSession>>>,
}

impl<B: StorageBackend> Storage<B> {
    /// Create new storage with backend
    pub fn new(backend: B) -> Self {
        Self {
            backend: Arc::new(backend),
            retained_cache: Arc::new(RwLock::new(HashMap::new())),
            sessions_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Initialize storage and load data from backend
    pub async fn initialize(&self) -> Result<()> {
        // Load retained messages into cache
        // Use "#" as wildcard to match all topics
        let retained_messages = self.backend.get_retained_messages("#").await?;
        let mut cache = self.retained_cache.write().await;
        for (topic, msg) in retained_messages {
            cache.insert(topic, msg);
        }
        
        // Load sessions into cache
        // Note: We can't enumerate all sessions without adding a new backend method,
        // so sessions will be loaded on-demand
        
        Ok(())
    }
    
    /// Store retained message
    pub async fn store_retained(&self, topic: &str, message: RetainedMessage) -> Result<()> {
        // Update cache
        self.retained_cache.write().await.insert(topic.to_string(), message.clone());
        
        // Persist to backend
        self.backend.store_retained_message(topic, message).await
    }
    
    /// Get retained message
    pub async fn get_retained(&self, topic: &str) -> Option<RetainedMessage> {
        let cache_result = self.retained_cache.read().await.get(topic).cloned();
        
        if let Some(msg) = cache_result {
            // Check if cached message is expired
            if msg.is_expired() {
                // Remove from cache and backend
                self.remove_retained(topic).await.ok();
                return None;
            }
            return Some(msg);
        }
        
        // Not in cache, try loading from backend
        if let Ok(Some(msg)) = self.backend.get_retained_message(topic).await {
            // Backend already checks expiry, so if we get here it's valid
            // Add to cache for future use
            self.retained_cache.write().await.insert(topic.to_string(), msg.clone());
            Some(msg)
        } else {
            None
        }
    }
    
    /// Remove retained message
    pub async fn remove_retained(&self, topic: &str) -> Result<()> {
        // Remove from cache
        self.retained_cache.write().await.remove(topic);
        
        // Remove from backend
        self.backend.remove_retained_message(topic).await
    }
    
    /// Get retained messages matching topic filter
    pub async fn get_retained_matching(&self, topic_filter: &str) -> Vec<(String, RetainedMessage)> {
        use crate::validation::topic_matches_filter;
        
        self.retained_cache.read().await
            .iter()
            .filter(|(topic, _)| topic_matches_filter(topic, topic_filter))
            .map(|(topic, msg)| (topic.clone(), msg.clone()))
            .collect()
    }
    
    /// Store client session
    pub async fn store_session(&self, session: ClientSession) -> Result<()> {
        let client_id = session.client_id.clone();
        
        // Update cache
        self.sessions_cache.write().await.insert(client_id, session.clone());
        
        // Persist to backend
        self.backend.store_session(session).await
    }
    
    /// Get client session
    pub async fn get_session(&self, client_id: &str) -> Option<ClientSession> {
        // First check cache
        let cache_result = self.sessions_cache.read().await.get(client_id).cloned();
        
        if let Some(session) = cache_result {
            // Check if cached session is expired
            if session.is_expired() {
                // Remove from cache and backend
                self.remove_session(client_id).await.ok();
                return None;
            }
            return Some(session);
        }
        
        // Not in cache, try loading from backend
        if let Ok(Some(session)) = self.backend.get_session(client_id).await {
            // Backend already checks expiry, so if we get here it's valid
            // Add to cache for future use
            self.sessions_cache.write().await.insert(client_id.to_string(), session.clone());
            Some(session)
        } else {
            None
        }
    }
    
    /// Remove client session
    pub async fn remove_session(&self, client_id: &str) -> Result<()> {
        // Remove from cache
        self.sessions_cache.write().await.remove(client_id);
        
        // Remove from backend
        self.backend.remove_session(client_id).await
    }
    
    /// Queue message for offline client
    pub async fn queue_message(&self, message: QueuedMessage) -> Result<()> {
        self.backend.queue_message(message).await
    }
    
    /// Get queued messages for client
    pub async fn get_queued_messages(&self, client_id: &str) -> Result<Vec<QueuedMessage>> {
        self.backend.get_queued_messages(client_id).await
    }
    
    /// Remove queued messages for client
    pub async fn remove_queued_messages(&self, client_id: &str) -> Result<()> {
        self.backend.remove_queued_messages(client_id).await
    }
    
    /// Periodic cleanup of expired data
    pub async fn cleanup_expired(&self) -> Result<()> {
        let now = SystemTime::now();
        
        // Clean expired retained messages from cache
        {
            let mut cache = self.retained_cache.write().await;
            cache.retain(|_, msg| {
                msg.expires_at.map_or(true, |expiry| expiry > now)
            });
        }
        
        // Clean expired sessions from cache
        {
            let mut cache = self.sessions_cache.write().await;
            cache.retain(|_, session| {
                if let Some(expiry_interval) = session.expiry_interval {
                    let expiry_time = session.last_seen + Duration::from_secs(expiry_interval as u64);
                    expiry_time > now
                } else {
                    true
                }
            });
        }
        
        // Clean expired data from backend
        self.backend.cleanup_expired().await
    }
}

impl RetainedMessage {
    /// Create new retained message from PUBLISH packet
    pub fn new(packet: PublishPacket) -> Self {
        let now = SystemTime::now();
        let expires_at = Self::extract_message_expiry(&packet)
            .map(|interval| now + Duration::from_secs(u64::from(interval)));
            
        Self {
            topic: packet.topic_name,
            payload: packet.payload,
            qos: packet.qos,
            retain: packet.retain,
            stored_at: now,
            expires_at,
        }
    }
    
    /// Convert to PublishPacket for delivery
    pub fn to_publish_packet(&self) -> PublishPacket {
        PublishPacket::new(&self.topic, self.payload.clone(), self.qos)
            .with_retain(self.retain)
    }
    
    /// Extract message expiry interval from packet properties
    fn extract_message_expiry(packet: &PublishPacket) -> Option<u32> {
        use crate::protocol::v5::properties::{PropertyId, PropertyValue};
        
        packet.properties.get(PropertyId::MessageExpiryInterval)
            .and_then(|prop| match prop {
                PropertyValue::FourByteInteger(value) => Some(*value),
                _ => None,
            })
    }
    
    /// Check if message has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|expiry| SystemTime::now() > expiry)
    }
}

impl ClientSession {
    /// Create new client session
    pub fn new(client_id: String, persistent: bool, expiry_interval: Option<u32>) -> Self {
        let now = SystemTime::now();
        Self {
            client_id,
            persistent,
            expiry_interval,
            subscriptions: HashMap::new(),
            created_at: now,
            last_seen: now,
        }
    }
    
    /// Update last seen time
    pub fn touch(&mut self) {
        self.last_seen = SystemTime::now();
    }
    
    /// Add subscription to session
    pub fn add_subscription(&mut self, topic_filter: String, qos: QoS) {
        self.subscriptions.insert(topic_filter, qos);
    }
    
    /// Remove subscription from session
    pub fn remove_subscription(&mut self, topic_filter: &str) {
        self.subscriptions.remove(topic_filter);
    }
    
    /// Check if session has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expiry_interval) = self.expiry_interval {
            let expiry_time = self.last_seen + Duration::from_secs(u64::from(expiry_interval));
            SystemTime::now() > expiry_time
        } else {
            false
        }
    }
}

impl QueuedMessage {
    /// Create new queued message from PUBLISH packet
    pub fn new(packet: PublishPacket, client_id: String, qos: QoS, packet_id: Option<u16>) -> Self {
        let now = SystemTime::now();
        let expires_at = Self::extract_message_expiry(&packet)
            .map(|interval| now + Duration::from_secs(u64::from(interval)));
            
        Self {
            topic: packet.topic_name,
            payload: packet.payload,
            client_id,
            qos,
            queued_at: now,
            expires_at,
            packet_id,
        }
    }
    
    /// Convert to PublishPacket for delivery
    pub fn to_publish_packet(&self) -> PublishPacket {
        let mut packet = PublishPacket::new(&self.topic, self.payload.clone(), self.qos);
        packet.packet_id = self.packet_id;
        packet
    }
    
    /// Extract message expiry interval from packet properties
    fn extract_message_expiry(packet: &PublishPacket) -> Option<u32> {
        use crate::protocol::v5::properties::{PropertyId, PropertyValue};
        
        packet.properties.get(PropertyId::MessageExpiryInterval)
            .and_then(|prop| match prop {
                PropertyValue::FourByteInteger(value) => Some(*value),
                _ => None,
            })
    }
    
    /// Check if message has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|expiry| SystemTime::now() > expiry)
    }
}

/// Dynamic storage backend that can hold different implementations
pub enum DynamicStorage {
    File(FileBackend),
    Memory(MemoryBackend),
}

impl StorageBackend for DynamicStorage {
    async fn store_retained_message(&self, topic: &str, message: RetainedMessage) -> Result<()> {
        match self {
            Self::File(backend) => backend.store_retained_message(topic, message).await,
            Self::Memory(backend) => backend.store_retained_message(topic, message).await,
        }
    }
    
    async fn get_retained_message(&self, topic: &str) -> Result<Option<RetainedMessage>> {
        match self {
            Self::File(backend) => backend.get_retained_message(topic).await,
            Self::Memory(backend) => backend.get_retained_message(topic).await,
        }
    }
    
    async fn remove_retained_message(&self, topic: &str) -> Result<()> {
        match self {
            Self::File(backend) => backend.remove_retained_message(topic).await,
            Self::Memory(backend) => backend.remove_retained_message(topic).await,
        }
    }
    
    async fn get_retained_messages(&self, topic_filter: &str) -> Result<Vec<(String, RetainedMessage)>> {
        match self {
            Self::File(backend) => backend.get_retained_messages(topic_filter).await,
            Self::Memory(backend) => backend.get_retained_messages(topic_filter).await,
        }
    }
    
    async fn store_session(&self, session: ClientSession) -> Result<()> {
        match self {
            Self::File(backend) => backend.store_session(session).await,
            Self::Memory(backend) => backend.store_session(session).await,
        }
    }
    
    async fn get_session(&self, client_id: &str) -> Result<Option<ClientSession>> {
        match self {
            Self::File(backend) => backend.get_session(client_id).await,
            Self::Memory(backend) => backend.get_session(client_id).await,
        }
    }
    
    async fn remove_session(&self, client_id: &str) -> Result<()> {
        match self {
            Self::File(backend) => backend.remove_session(client_id).await,
            Self::Memory(backend) => backend.remove_session(client_id).await,
        }
    }
    
    async fn queue_message(&self, message: QueuedMessage) -> Result<()> {
        match self {
            Self::File(backend) => backend.queue_message(message).await,
            Self::Memory(backend) => backend.queue_message(message).await,
        }
    }
    
    async fn get_queued_messages(&self, client_id: &str) -> Result<Vec<QueuedMessage>> {
        match self {
            Self::File(backend) => backend.get_queued_messages(client_id).await,
            Self::Memory(backend) => backend.get_queued_messages(client_id).await,
        }
    }
    
    async fn remove_queued_messages(&self, client_id: &str) -> Result<()> {
        match self {
            Self::File(backend) => backend.remove_queued_messages(client_id).await,
            Self::Memory(backend) => backend.remove_queued_messages(client_id).await,
        }
    }
    
    async fn cleanup_expired(&self) -> Result<()> {
        match self {
            Self::File(backend) => backend.cleanup_expired().await,
            Self::Memory(backend) => backend.cleanup_expired().await,
        }
    }
}