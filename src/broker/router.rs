//! Message routing for the MQTT broker
//! 
//! Routes messages between clients based on subscriptions

use crate::packet::publish::PublishPacket;
use crate::validation::topic_matches_filter;
use crate::QoS;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, trace};

/// Client subscription information
#[derive(Debug, Clone)]
pub struct Subscription {
    /// Client ID that owns this subscription
    pub client_id: String,
    /// Quality of Service level
    pub qos: QoS,
    /// Subscription identifier (MQTT v5.0)
    pub subscription_id: Option<u32>,
}

/// Message router for the broker
pub struct MessageRouter {
    /// Map of topic filters to subscriptions
    subscriptions: Arc<RwLock<HashMap<String, Vec<Subscription>>>>,
    /// Map of exact topics to retained messages
    retained_messages: Arc<RwLock<HashMap<String, PublishPacket>>>,
    /// Active client connections
    clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
}

/// Information about a connected client
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// Channel to send messages to this client
    pub sender: tokio::sync::mpsc::Sender<PublishPacket>,
}

impl MessageRouter {
    /// Creates a new message router
    #[must_use]
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            retained_messages: Arc::new(RwLock::new(HashMap::new())),
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Registers a client connection
    pub async fn register_client(&self, client_id: String, sender: tokio::sync::mpsc::Sender<PublishPacket>) {
        let mut clients = self.clients.write().await;
        clients.insert(client_id.clone(), ClientInfo { sender });
        debug!("Registered client: {}", client_id);
    }
    
    /// Unregisters a client connection
    pub async fn unregister_client(&self, client_id: &str) {
        let mut clients = self.clients.write().await;
        clients.remove(client_id);
        
        // Remove all subscriptions for this client
        let mut subscriptions = self.subscriptions.write().await;
        for subs in subscriptions.values_mut() {
            subs.retain(|sub| sub.client_id != client_id);
        }
        
        // Clean up empty subscription lists
        subscriptions.retain(|_, subs| !subs.is_empty());
        
        debug!("Unregistered client: {}", client_id);
    }
    
    /// Adds a subscription for a client
    pub async fn subscribe(&self, client_id: String, topic_filter: String, qos: QoS, subscription_id: Option<u32>) {
        let mut subscriptions = self.subscriptions.write().await;
        let subscription = Subscription {
            client_id: client_id.clone(),
            qos,
            subscription_id,
        };
        
        subscriptions
            .entry(topic_filter.clone())
            .or_default()
            .push(subscription);
        
        debug!("Client {} subscribed to {}", client_id, topic_filter);
    }
    
    /// Removes a subscription for a client
    pub async fn unsubscribe(&self, client_id: &str, topic_filter: &str) -> bool {
        let mut subscriptions = self.subscriptions.write().await;
        
        if let Some(subs) = subscriptions.get_mut(topic_filter) {
            let initial_len = subs.len();
            subs.retain(|sub| sub.client_id != client_id);
            
            let removed = initial_len != subs.len();
            
            // Remove empty entries after calculating removed flag
            if subs.is_empty() {
                subscriptions.remove(topic_filter);
            }
            if removed {
                debug!("Client {} unsubscribed from {}", client_id, topic_filter);
            }
            removed
        } else {
            false
        }
    }
    
    /// Routes a publish message to all matching subscribers
    pub async fn route_message(&self, publish: &PublishPacket) {
        trace!("Routing message to topic: {}", publish.topic_name);
        
        // Handle retained messages
        if publish.retain {
            let mut retained = self.retained_messages.write().await;
            if publish.payload.is_empty() {
                // Empty payload means delete retained message
                retained.remove(&publish.topic_name);
                debug!("Deleted retained message for topic: {}", publish.topic_name);
            } else {
                retained.insert(publish.topic_name.clone(), publish.clone());
                debug!("Stored retained message for topic: {}", publish.topic_name);
            }
        }
        
        // Find matching subscriptions
        let subscriptions = self.subscriptions.read().await;
        let clients = self.clients.read().await;
        
        for (topic_filter, subs) in subscriptions.iter() {
            if topic_matches_filter(&publish.topic_name, topic_filter) {
                for sub in subs {
                    if let Some(client_info) = clients.get(&sub.client_id) {
                        // Calculate effective QoS (minimum of publish and subscription QoS)
                        let effective_qos = match (publish.qos, sub.qos) {
                            (QoS::AtMostOnce, _) | (_, QoS::AtMostOnce) => QoS::AtMostOnce,
                            (QoS::AtLeastOnce | QoS::ExactlyOnce, QoS::AtLeastOnce) | (QoS::AtLeastOnce, QoS::ExactlyOnce) => QoS::AtLeastOnce,
                            (QoS::ExactlyOnce, QoS::ExactlyOnce) => QoS::ExactlyOnce,
                        };
                        
                        // Create message with effective QoS
                        let mut message = publish.clone();
                        message.qos = effective_qos;
                        
                        // Add subscription identifier if present
                        if let Some(id) = sub.subscription_id {
                            message.properties.set_subscription_identifier(id);
                        }
                        
                        // Send to client (non-blocking)
                        let _ = client_info.sender.try_send(message);
                    }
                }
            }
        }
    }
    
    /// Gets retained messages matching a topic filter
    pub async fn get_retained_messages(&self, topic_filter: &str) -> Vec<PublishPacket> {
        let retained = self.retained_messages.read().await;
        retained
            .iter()
            .filter(|(topic, _)| topic_matches_filter(topic, topic_filter))
            .map(|(_, msg)| msg.clone())
            .collect()
    }
    
    /// Gets the number of connected clients
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }
    
    /// Gets the number of unique topic filters with subscriptions
    pub async fn topic_count(&self) -> usize {
        self.subscriptions.read().await.len()
    }
    
    /// Gets the number of retained messages
    pub async fn retained_count(&self) -> usize {
        self.retained_messages.read().await.len()
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_client_registration() {
        let router = MessageRouter::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(100);
        
        router.register_client("client1".to_string(), tx).await;
        assert_eq!(router.client_count().await, 1);
        
        router.unregister_client("client1").await;
        assert_eq!(router.client_count().await, 0);
    }
    
    #[tokio::test]
    async fn test_subscription_management() {
        let router = MessageRouter::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(100);
        
        router.register_client("client1".to_string(), tx).await;
        router.subscribe("client1".to_string(), "test/+".to_string(), QoS::AtLeastOnce, None).await;
        
        assert_eq!(router.topic_count().await, 1);
        
        let removed = router.unsubscribe("client1", "test/+").await;
        assert!(removed);
        assert_eq!(router.topic_count().await, 0);
    }
    
    #[tokio::test]
    async fn test_message_routing() {
        let router = MessageRouter::new();
        let (tx1, mut rx1) = tokio::sync::mpsc::channel(100);
        let (tx2, mut rx2) = tokio::sync::mpsc::channel(100);
        
        // Register clients
        router.register_client("client1".to_string(), tx1).await;
        router.register_client("client2".to_string(), tx2).await;
        
        // Subscribe to different patterns
        router.subscribe("client1".to_string(), "test/+".to_string(), QoS::AtLeastOnce, None).await;
        router.subscribe("client2".to_string(), "test/data".to_string(), QoS::ExactlyOnce, None).await;
        
        // Publish message
        let publish = PublishPacket::new("test/data", b"hello", QoS::ExactlyOnce);
        
        router.route_message(&publish).await;
        
        // Client 1 should receive with QoS 1 (downgraded)
        let msg1 = rx1.try_recv().unwrap();
        assert_eq!(msg1.topic_name, "test/data");
        assert_eq!(msg1.qos, QoS::AtLeastOnce);
        
        // Client 2 should receive with QoS 2
        let msg2 = rx2.try_recv().unwrap();
        assert_eq!(msg2.topic_name, "test/data");
        assert_eq!(msg2.qos, QoS::ExactlyOnce);
    }
    
    #[tokio::test]
    async fn test_retained_messages() {
        let router = MessageRouter::new();
        
        // Store retained message
        let mut publish = PublishPacket::new("test/status", b"online", QoS::AtMostOnce);
        publish.retain = true;
        router.route_message(&publish).await;
        
        assert_eq!(router.retained_count().await, 1);
        
        // Get retained messages
        let retained = router.get_retained_messages("test/+").await;
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].topic_name, "test/status");
        
        // Delete retained message
        let mut delete = PublishPacket::new("test/status", b"", QoS::AtMostOnce);
        delete.retain = true;
        router.route_message(&delete).await;
        
        assert_eq!(router.retained_count().await, 0);
    }
}