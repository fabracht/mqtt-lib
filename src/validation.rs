use crate::error::{MqttError, Result};

/// Validates an MQTT topic name according to MQTT v5.0 specification
///
/// # Rules:
/// - Must be UTF-8 encoded
/// - Must have at least one character
/// - Must not contain null characters (U+0000)
/// - Must not exceed maximum string length when UTF-8 encoded
/// - Should not contain wildcard characters (+, #) in topic names (only in filters)
#[must_use]
pub fn is_valid_topic_name(topic: &str) -> bool {
    if topic.is_empty() {
        return false;
    }

    if topic.len() > crate::constants::limits::MAX_STRING_LENGTH as usize {
        return false;
    }

    if topic.contains('\0') {
        return false;
    }

    // Topic names should not contain wildcards
    if topic.contains('+') || topic.contains('#') {
        return false;
    }

    true
}

/// Validates an MQTT topic filter according to MQTT v5.0 specification
///
/// # Rules:
/// - Must follow all topic name rules except wildcard usage
/// - Single-level wildcard (+) must occupy entire level
/// - Multi-level wildcard (#) must be last character and occupy entire level
/// - Examples: sport/+/player, sport/tennis/#, +/tennis/#
#[must_use]
pub fn is_valid_topic_filter(filter: &str) -> bool {
    if filter.is_empty() {
        return false;
    }

    if filter.len() > crate::constants::limits::MAX_STRING_LENGTH as usize {
        return false;
    }

    if filter.contains('\0') {
        return false;
    }

    let parts: Vec<&str> = filter.split('/').collect();

    for (i, part) in parts.iter().enumerate() {
        // Multi-level wildcard rules
        if part.contains('#') {
            // # must be the last character in the filter
            if i != parts.len() - 1 {
                return false;
            }
            // # must occupy the entire level
            if *part != "#" {
                return false;
            }
        }

        // Single-level wildcard rules
        if part.contains('+') {
            // + must occupy the entire level
            if *part != "+" {
                return false;
            }
        }
    }

    true
}

/// Validates an MQTT client identifier according to MQTT v5.0 specification
///
/// # Rules:
/// - Must be UTF-8 encoded
/// - Must contain only characters: 0-9, a-z, A-Z
/// - Must be between 1 and 23 bytes (unless server supports longer)
/// - Empty string is allowed (server will assign one)
#[must_use]
pub fn is_valid_client_id(client_id: &str) -> bool {
    if client_id.is_empty() {
        return true; // Empty client ID is allowed
    }

    if client_id.len() > 23 {
        // Most servers support longer, but 23 is the spec minimum
        // We'll allow longer and let the server reject if needed
        if client_id.len() > crate::constants::limits::MAX_CLIENT_ID_LENGTH {
            return false; // Reasonable upper limit
        }
    }

    // Check for valid characters (alphanumeric)
    client_id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Validates a topic name and returns an error if invalid
///
/// # Errors
///
/// Returns `MqttError::InvalidTopicName` if the topic name:
/// - Is empty
/// - Exceeds maximum string length
/// - Contains null characters
/// - Contains wildcard characters (+, #)
pub fn validate_topic_name(topic: &str) -> Result<()> {
    if !is_valid_topic_name(topic) {
        return Err(MqttError::InvalidTopicName(topic.to_string()));
    }
    Ok(())
}

/// Validates a topic filter and returns an error if invalid
///
/// # Errors
///
/// Returns `MqttError::InvalidTopicFilter` if the topic filter:
/// - Is empty
/// - Exceeds maximum string length
/// - Contains null characters
/// - Has invalid wildcard usage
pub fn validate_topic_filter(filter: &str) -> Result<()> {
    if !is_valid_topic_filter(filter) {
        return Err(MqttError::InvalidTopicFilter(filter.to_string()));
    }
    Ok(())
}

/// Validates a client ID and returns an error if invalid
///
/// # Errors
///
/// Returns `MqttError::InvalidClientId` if the client ID:
/// - Contains non-alphanumeric characters
/// - Exceeds maximum client ID length
pub fn validate_client_id(client_id: &str) -> Result<()> {
    if !is_valid_client_id(client_id) {
        return Err(MqttError::InvalidClientId(client_id.to_string()));
    }
    Ok(())
}

/// Checks if a topic name matches a topic filter
///
/// # Rules:
/// - '+' matches exactly one topic level
/// - '#' matches any number of levels including parent level
/// - Topic and filter level separators must match exactly
#[must_use]
pub fn topic_matches_filter(topic: &str, filter: &str) -> bool {
    if filter == "#" {
        return true;
    }

    let topic_parts: Vec<&str> = topic.split('/').collect();
    let filter_parts: Vec<&str> = filter.split('/').collect();

    let mut t_idx = 0;
    let mut f_idx = 0;

    while t_idx < topic_parts.len() && f_idx < filter_parts.len() {
        if filter_parts[f_idx] == "#" {
            return true; // Multi-level wildcard matches everything remaining
        }

        if filter_parts[f_idx] != "+" && filter_parts[f_idx] != topic_parts[t_idx] {
            return false; // Not a match
        }

        t_idx += 1;
        f_idx += 1;
    }

    // Check if we've consumed both topic and filter
    if t_idx == topic_parts.len() && f_idx == filter_parts.len() {
        return true;
    }

    // Check if filter ends with # and we've consumed the topic
    if t_idx == topic_parts.len() && f_idx == filter_parts.len() - 1 && filter_parts[f_idx] == "#" {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_topic_names() {
        assert!(is_valid_topic_name("sport/tennis"));
        assert!(is_valid_topic_name("sport/tennis/player1"));
        assert!(is_valid_topic_name("home/temperature"));
        assert!(is_valid_topic_name("/"));
        assert!(is_valid_topic_name("a"));
    }

    #[test]
    fn test_invalid_topic_names() {
        assert!(!is_valid_topic_name(""));
        assert!(!is_valid_topic_name("sport/+/player"));
        assert!(!is_valid_topic_name("sport/tennis/#"));
        assert!(!is_valid_topic_name("home\0temperature"));

        let too_long = "a".repeat(crate::constants::limits::MAX_BINARY_LENGTH as usize);
        assert!(!is_valid_topic_name(&too_long));
    }

    #[test]
    fn test_valid_topic_filters() {
        assert!(is_valid_topic_filter("sport/tennis"));
        assert!(is_valid_topic_filter("sport/+/player"));
        assert!(is_valid_topic_filter("sport/tennis/#"));
        assert!(is_valid_topic_filter("#"));
        assert!(is_valid_topic_filter("+"));
        assert!(is_valid_topic_filter("+/tennis/#"));
        assert!(is_valid_topic_filter("sport/+"));
    }

    #[test]
    fn test_invalid_topic_filters() {
        assert!(!is_valid_topic_filter(""));
        assert!(!is_valid_topic_filter("sport/tennis#"));
        assert!(!is_valid_topic_filter("sport/tennis/#/ranking"));
        assert!(!is_valid_topic_filter("sport+"));
        assert!(!is_valid_topic_filter("sport/+tennis"));
        assert!(!is_valid_topic_filter("home\0temperature"));
    }

    #[test]
    fn test_valid_client_ids() {
        assert!(is_valid_client_id(""));
        assert!(is_valid_client_id("client123"));
        assert!(is_valid_client_id("MyClient"));
        assert!(is_valid_client_id("123456789012345678901234"));
        assert!(is_valid_client_id("a1b2c3d4e5f6"));
    }

    #[test]
    fn test_invalid_client_ids() {
        assert!(!is_valid_client_id("client-123"));
        assert!(!is_valid_client_id("client.123"));
        assert!(!is_valid_client_id("client 123"));
        assert!(!is_valid_client_id("client@123"));

        let too_long = "a".repeat(crate::constants::limits::MAX_CLIENT_ID_LENGTH + 1);
        assert!(!is_valid_client_id(&too_long));
    }

    #[test]
    fn test_topic_matches_filter() {
        // Exact matches
        assert!(topic_matches_filter("sport/tennis", "sport/tennis"));

        // Single-level wildcard
        assert!(topic_matches_filter("sport/tennis", "sport/+"));
        assert!(topic_matches_filter(
            "sport/tennis/player1",
            "sport/+/player1"
        ));
        assert!(topic_matches_filter(
            "sport/tennis/player1",
            "sport/tennis/+"
        ));
        assert!(!topic_matches_filter("sport/tennis/player1", "sport/+"));

        // Multi-level wildcard
        assert!(topic_matches_filter("sport/tennis", "sport/#"));
        assert!(topic_matches_filter("sport/tennis/player1", "sport/#"));
        assert!(topic_matches_filter(
            "sport/tennis/player1/ranking",
            "sport/#"
        ));
        assert!(topic_matches_filter("sport", "sport/#"));
        assert!(topic_matches_filter("anything", "#"));
        assert!(topic_matches_filter("sport/tennis", "#"));

        // Non-matches
        assert!(!topic_matches_filter("sport/tennis", "sport/football"));
        assert!(!topic_matches_filter("sport", "sport/tennis"));
        assert!(!topic_matches_filter(
            "sport/tennis/player1",
            "sport/tennis"
        ));
    }
}
