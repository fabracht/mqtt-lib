//! Authentication and authorization for the MQTT broker

use crate::error::Result;
use crate::packet::connect::ConnectPacket;
use crate::protocol::v5::reason_codes::ReasonCode;
use async_trait::async_trait;
use std::net::SocketAddr;

/// Authentication result from an auth provider
#[derive(Debug, Clone)]
pub struct AuthResult {
    /// Whether authentication succeeded
    pub authenticated: bool,
    /// Reason code for `ConnAck`
    pub reason_code: ReasonCode,
    /// Optional reason string
    pub reason_string: Option<String>,
    /// User identifier after successful auth
    pub user_id: Option<String>,
}

impl AuthResult {
    /// Creates a successful authentication result
    #[must_use]
    pub fn success() -> Self {
        Self {
            authenticated: true,
            reason_code: ReasonCode::Success,
            reason_string: None,
            user_id: None,
        }
    }
    
    /// Creates a successful authentication result with user ID
    #[must_use]
    pub fn success_with_user(user_id: String) -> Self {
        Self {
            authenticated: true,
            reason_code: ReasonCode::Success,
            reason_string: None,
            user_id: Some(user_id),
        }
    }
    
    /// Creates a failed authentication result
    #[must_use]
    pub fn fail(reason_code: ReasonCode) -> Self {
        Self {
            authenticated: false,
            reason_code,
            reason_string: None,
            user_id: None,
        }
    }
    
    /// Creates a failed authentication result with reason string
    #[must_use]
    pub fn fail_with_reason(reason_code: ReasonCode, reason: String) -> Self {
        Self {
            authenticated: false,
            reason_code,
            reason_string: Some(reason),
            user_id: None,
        }
    }
}

/// Authentication provider trait
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Authenticate a client connection
    /// 
    /// # Errors
    /// 
    /// Returns an error if authentication check fails (not auth failure)
    async fn authenticate(
        &self,
        connect: &ConnectPacket,
        client_addr: SocketAddr,
    ) -> Result<AuthResult>;
    
    /// Check if a client is authorized to publish to a topic
    /// 
    /// # Errors
    /// 
    /// Returns an error if authorization check fails
    async fn authorize_publish(
        &self,
        client_id: &str,
        user_id: Option<&str>,
        topic: &str,
    ) -> Result<bool>;
    
    /// Check if a client is authorized to subscribe to a topic filter
    /// 
    /// # Errors
    /// 
    /// Returns an error if authorization check fails
    async fn authorize_subscribe(
        &self,
        client_id: &str,
        user_id: Option<&str>,
        topic_filter: &str,
    ) -> Result<bool>;
}

/// Allow all authentication provider (for testing/development)
#[derive(Debug, Clone, Default)]
pub struct AllowAllAuthProvider;

#[async_trait]
impl AuthProvider for AllowAllAuthProvider {
    async fn authenticate(
        &self,
        _connect: &ConnectPacket,
        _client_addr: SocketAddr,
    ) -> Result<AuthResult> {
        Ok(AuthResult::success())
    }
    
    async fn authorize_publish(
        &self,
        _client_id: &str,
        _user_id: Option<&str>,
        _topic: &str,
    ) -> Result<bool> {
        Ok(true)
    }
    
    async fn authorize_subscribe(
        &self,
        _client_id: &str,
        _user_id: Option<&str>,
        _topic_filter: &str,
    ) -> Result<bool> {
        Ok(true)
    }
}

/// Simple username/password authentication provider
#[derive(Debug, Clone)]
pub struct PasswordAuthProvider {
    /// Map of username to password
    users: std::collections::HashMap<String, String>,
}

impl PasswordAuthProvider {
    /// Creates a new password auth provider
    #[must_use]
    pub fn new() -> Self {
        Self {
            users: std::collections::HashMap::new(),
        }
    }
    
    /// Adds a user with password
    pub fn add_user(&mut self, username: String, password: String) {
        self.users.insert(username, password);
    }
    
    /// Removes a user
    pub fn remove_user(&mut self, username: &str) -> bool {
        self.users.remove(username).is_some()
    }
}

impl Default for PasswordAuthProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuthProvider for PasswordAuthProvider {
    async fn authenticate(
        &self,
        connect: &ConnectPacket,
        _client_addr: SocketAddr,
    ) -> Result<AuthResult> {
        // Check if username is provided
        let Some(username) = &connect.username else {
            return Ok(AuthResult::fail(ReasonCode::BadUsernameOrPassword));
        };
        
        // Check if password is provided
        let Some(password) = &connect.password else {
            return Ok(AuthResult::fail(ReasonCode::BadUsernameOrPassword));
        };
        
        // Verify username/password
        match self.users.get(username) {
            Some(expected_password) if expected_password.as_bytes() == password.as_slice() => {
                Ok(AuthResult::success_with_user(username.clone()))
            }
            _ => Ok(AuthResult::fail(ReasonCode::BadUsernameOrPassword)),
        }
    }
    
    async fn authorize_publish(
        &self,
        _client_id: &str,
        _user_id: Option<&str>,
        _topic: &str,
    ) -> Result<bool> {
        // Simple provider allows all authenticated users to publish anywhere
        Ok(true)
    }
    
    async fn authorize_subscribe(
        &self,
        _client_id: &str,
        _user_id: Option<&str>,
        _topic_filter: &str,
    ) -> Result<bool> {
        // Simple provider allows all authenticated users to subscribe anywhere
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::connect::ConnectPacket;
    use crate::types::ConnectOptions;
    
    #[tokio::test]
    async fn test_allow_all_provider() {
        let provider = AllowAllAuthProvider;
        let connect = ConnectPacket::new(ConnectOptions::new("test-client"));
        let addr = "127.0.0.1:12345".parse().unwrap();
        
        let result = provider.authenticate(&connect, addr).await.unwrap();
        assert!(result.authenticated);
        assert_eq!(result.reason_code, ReasonCode::Success);
        
        let can_publish = provider.authorize_publish("test", None, "test/topic").await.unwrap();
        assert!(can_publish);
        
        let can_subscribe = provider.authorize_subscribe("test", None, "test/+").await.unwrap();
        assert!(can_subscribe);
    }
    
    #[tokio::test]
    async fn test_password_provider() {
        let mut provider = PasswordAuthProvider::new();
        provider.add_user("alice".to_string(), "secret123".to_string());
        
        let addr = "127.0.0.1:12345".parse().unwrap();
        
        // Test successful auth
        let mut connect = ConnectPacket::new(ConnectOptions::new("test-client"));
        connect.username = Some("alice".to_string());
        connect.password = Some("secret123".as_bytes().to_vec());
        
        let result = provider.authenticate(&connect, addr).await.unwrap();
        assert!(result.authenticated);
        assert_eq!(result.user_id, Some("alice".to_string()));
        
        // Test wrong password
        connect.password = Some("wrong".as_bytes().to_vec());
        let result = provider.authenticate(&connect, addr).await.unwrap();
        assert!(!result.authenticated);
        assert_eq!(result.reason_code, ReasonCode::BadUsernameOrPassword);
        
        // Test missing password
        connect.password = None;
        let result = provider.authenticate(&connect, addr).await.unwrap();
        assert!(!result.authenticated);
        
        // Test unknown user
        connect.username = Some("bob".to_string());
        connect.password = Some("password".as_bytes().to_vec());
        let result = provider.authenticate(&connect, addr).await.unwrap();
        assert!(!result.authenticated);
    }
    
    #[test]
    fn test_auth_result_builders() {
        let result = AuthResult::success();
        assert!(result.authenticated);
        assert_eq!(result.reason_code, ReasonCode::Success);
        assert!(result.user_id.is_none());
        
        let result = AuthResult::success_with_user("alice".to_string());
        assert!(result.authenticated);
        assert_eq!(result.user_id, Some("alice".to_string()));
        
        let result = AuthResult::fail(ReasonCode::NotAuthorized);
        assert!(!result.authenticated);
        assert_eq!(result.reason_code, ReasonCode::NotAuthorized);
        
        let result = AuthResult::fail_with_reason(
            ReasonCode::Banned,
            "Too many failed attempts".to_string()
        );
        assert!(!result.authenticated);
        assert_eq!(result.reason_code, ReasonCode::Banned);
        assert_eq!(result.reason_string, Some("Too many failed attempts".to_string()));
    }
}