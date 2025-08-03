# Shared Subscriptions

Shared subscriptions are an MQTT v5.0 feature that allows multiple clients to share the load of processing messages published to a topic. This is particularly useful for implementing worker queues and load balancing scenarios.

## How it Works

When multiple clients subscribe to a shared subscription, the broker ensures that each message is delivered to only one of the subscribers in the group. This enables automatic load balancing across multiple consumers.

## Syntax

Shared subscriptions use the following topic filter format:

```
$share/<ShareGroupName>/<TopicFilter>
```

- `$share` - Fixed prefix indicating a shared subscription
- `<ShareGroupName>` - Name of the share group (alphanumeric)
- `<TopicFilter>` - Standard MQTT topic filter (can include wildcards)

## Examples

### Basic Worker Queue

```rust
// Three workers subscribe to the same shared subscription
client1.subscribe("$share/workers/tasks/+", callback).await?;
client2.subscribe("$share/workers/tasks/+", callback).await?;
client3.subscribe("$share/workers/tasks/+", callback).await?;

// Messages published to tasks/job1, tasks/job2, etc. will be
// distributed among the three workers
```

### Multiple Share Groups

Different groups can independently share the same topic:

```rust
// Team A workers
client_a1.subscribe("$share/team-a/alerts/critical", callback).await?;
client_a2.subscribe("$share/team-a/alerts/critical", callback).await?;

// Team B workers  
client_b1.subscribe("$share/team-b/alerts/critical", callback).await?;
client_b2.subscribe("$share/team-b/alerts/critical", callback).await?;

// Each team gets a copy of every alert, but within each team
// only one member processes each alert
```

### Mixed Subscriptions

Shared and regular subscriptions can coexist:

```rust
// Shared subscription - only one worker gets each message
worker1.subscribe("$share/workers/orders/new", callback).await?;
worker2.subscribe("$share/workers/orders/new", callback).await?;

// Regular subscription - monitor gets ALL messages
monitor.subscribe("orders/new", callback).await?;
```

## Load Balancing

The broker uses round-robin distribution to ensure fair message distribution among active subscribers in a share group. If a subscriber disconnects, its messages are automatically redistributed to remaining group members.

## Use Cases

1. **Work Queues**: Distribute tasks among multiple workers
2. **Microservices**: Load balance requests across service instances  
3. **High Availability**: Ensure messages are processed even if some consumers fail
4. **Scalability**: Add/remove workers dynamically based on load

## Implementation Notes

- Messages are distributed only to currently connected subscribers
- If all subscribers in a group are offline, behavior depends on QoS:
  - QoS 0: Messages are dropped
  - QoS 1/2: Messages are queued for the first subscriber to reconnect
- Each share group maintains independent round-robin state
- Subscription identifiers work normally with shared subscriptions