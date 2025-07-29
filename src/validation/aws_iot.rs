use crate::error::{MqttError, Result};
use crate::validation::{validate_topic_filter, validate_topic_name, TopicValidator};

/// AWS IoT specific topic validator
///
/// This validator implements AWS IoT Core specific rules while maintaining
/// vendor neutrality by not depending on AWS APIs or SDK components.
///
/// AWS IoT reserved topics include:
/// - `$aws/` - AWS IoT service topics
/// - `$SYS/` - System topics (MQTT broker information)
///
/// Optional device-specific validation can be enabled by setting the device name,
/// which allows topics like `$aws/thing/{device_name}/*` while rejecting others.
#[derive(Debug, Clone)]
pub struct AwsIotValidator {
    /// Optional device name for device-specific `$aws/thing/` validation
    /// If set, allows `$aws/thing/{device_name}/*` topics
    pub device_name: Option<String>,
    /// Whether to allow system topics (`$SYS/*`)
    pub allow_system_topics: bool,
    /// Additional reserved prefixes beyond AWS defaults
    pub additional_reserved_prefixes: Vec<String>,
}

impl Default for AwsIotValidator {
    fn default() -> Self {
        Self {
            device_name: None,
            allow_system_topics: false,
            additional_reserved_prefixes: Vec::new(),
        }
    }
}

impl AwsIotValidator {
    /// Creates a new AWS IoT validator
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the device name for device-specific topic validation
    ///
    /// When set, allows topics like `$aws/thing/{device_name}/*` while
    /// rejecting `$aws/thing/{other_device}/*`
    #[must_use]
    pub fn with_device_name(mut self, device_name: impl Into<String>) -> Self {
        self.device_name = Some(device_name.into());
        self
    }

    /// Enables system topics (`$SYS/*`)
    #[must_use]
    pub fn with_system_topics(mut self, allow: bool) -> Self {
        self.allow_system_topics = allow;
        self
    }

    /// Adds an additional reserved prefix
    #[must_use]
    pub fn with_reserved_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.additional_reserved_prefixes.push(prefix.into());
        self
    }

    /// Checks if a topic is an AWS service topic (`$aws/*`)
    fn is_aws_service_topic(&self, topic: &str) -> bool {
        topic.starts_with("$aws/")
    }

    /// Checks if a topic is a system topic (`$SYS/*`)  
    fn is_system_topic(&self, topic: &str) -> bool {
        topic.starts_with("$SYS/")
    }

    /// Validates AWS-specific topic restrictions
    fn validate_aws_restrictions(&self, topic: &str) -> Result<()> {
        // Check system topics
        if self.is_system_topic(topic) && !self.allow_system_topics {
            return Err(MqttError::InvalidTopicName(
                "System topics ($SYS/*) are not allowed".to_string(),
            ));
        }

        // Check AWS service topics
        if self.is_aws_service_topic(topic) {
            // If we have a device name, check device-specific topics
            if let Some(ref device_name) = self.device_name {
                let device_prefix = format!("$aws/thing/{}/", device_name);

                // Allow device-specific topics
                if topic.starts_with(&device_prefix) {
                    return Ok(());
                }

                // Allow general AWS topics that don't start with $aws/thing/
                if !topic.starts_with("$aws/thing/") {
                    // These are general AWS service topics like $aws/events, $aws/rules, etc.
                    // Allow them for now, but this could be further restricted based on requirements
                    return Ok(());
                }

                // Reject topics for other devices
                if topic.starts_with("$aws/thing/") {
                    return Err(MqttError::InvalidTopicName(format!(
                        "Topic '{}' is for a different device. Only topics under '$aws/thing/{}/' are allowed",
                        topic, device_name
                    )));
                }
            } else {
                // No device name set - reject all device-specific topics
                if topic.starts_with("$aws/thing/") {
                    return Err(MqttError::InvalidTopicName(
                        "Device-specific topics ($aws/thing/*) require device name to be configured".to_string(),
                    ));
                }
                // Allow other AWS service topics
            }
        }

        // Check additional reserved prefixes
        for prefix in &self.additional_reserved_prefixes {
            if topic.starts_with(prefix) {
                return Err(MqttError::InvalidTopicName(format!(
                    "Topic '{}' uses reserved prefix '{}'",
                    topic, prefix
                )));
            }
        }

        Ok(())
    }
}

impl TopicValidator for AwsIotValidator {
    fn validate_topic_name(&self, topic: &str) -> Result<()> {
        // First apply standard MQTT validation
        validate_topic_name(topic)?;

        // Then apply AWS IoT specific restrictions
        self.validate_aws_restrictions(topic)
    }

    fn validate_topic_filter(&self, filter: &str) -> Result<()> {
        // First apply standard MQTT validation
        validate_topic_filter(filter)?;

        // For filters, we're more lenient with wildcards
        // Only check if it's a literal reserved prefix (no wildcards)
        if !filter.contains('+') && !filter.contains('#') {
            self.validate_aws_restrictions(filter)?;
        }

        Ok(())
    }

    fn is_reserved_topic(&self, topic: &str) -> bool {
        // Check AWS service topics
        if self.is_aws_service_topic(topic) {
            // If we have a device name, only topics outside our device are reserved
            if let Some(ref device_name) = self.device_name {
                let device_prefix = format!("$aws/thing/{}/", device_name);
                return !topic.starts_with(&device_prefix) && topic.starts_with("$aws/thing/");
            }
            // If no device name, all $aws/thing/ topics are reserved
            return topic.starts_with("$aws/thing/");
        }

        // Check system topics
        if self.is_system_topic(topic) && !self.allow_system_topics {
            return true;
        }

        // Check additional reserved prefixes
        self.additional_reserved_prefixes
            .iter()
            .any(|prefix| topic.starts_with(prefix))
    }

    fn description(&self) -> &'static str {
        "AWS IoT Core specific topic validator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aws_iot_validator_basic() {
        let validator = AwsIotValidator::new();

        // Regular topics should work
        assert!(validator.validate_topic_name("sensor/temperature").is_ok());
        assert!(validator.validate_topic_filter("sensor/+").is_ok());

        // System topics should be rejected by default
        assert!(validator
            .validate_topic_name("$SYS/broker/version")
            .is_err());
        assert!(!validator.is_reserved_topic("regular/topic"));
        assert!(validator.is_reserved_topic("$SYS/broker/version"));
    }

    #[test]
    fn test_aws_iot_validator_with_device() {
        let validator = AwsIotValidator::new().with_device_name("my-device");

        // Device-specific topics should work
        assert!(validator
            .validate_topic_name("$aws/thing/my-device/shadow/update")
            .is_ok());

        // Other device topics should be rejected
        assert!(validator
            .validate_topic_name("$aws/thing/other-device/shadow/update")
            .is_err());

        // General AWS topics should work
        assert!(validator
            .validate_topic_name("$aws/events/presence/connected/my-device")
            .is_ok());
    }

    #[test]
    fn test_aws_iot_validator_system_topics() {
        let validator = AwsIotValidator::new().with_system_topics(true);

        // System topics should now work
        assert!(validator.validate_topic_name("$SYS/broker/version").is_ok());
        assert!(!validator.is_reserved_topic("$SYS/broker/version"));
    }

    #[test]
    fn test_aws_iot_validator_additional_prefixes() {
        let validator = AwsIotValidator::new()
            .with_reserved_prefix("company/")
            .with_reserved_prefix("internal/");

        // Additional reserved prefixes should be rejected
        assert!(validator.validate_topic_name("company/secret").is_err());
        assert!(validator.validate_topic_name("internal/admin").is_err());
        assert!(validator.is_reserved_topic("company/secret"));

        // Regular topics should work
        assert!(validator.validate_topic_name("public/sensor").is_ok());
    }
}
