# Production Deployment

Deploying the MQTT broker in production environments.

## Pre-Deployment Checklist

### Security
- [ ] Disable anonymous access
- [ ] Configure strong authentication
- [ ] Enable TLS/SSL encryption
- [ ] Set up ACL rules
- [ ] Configure firewall rules
- [ ] Review password security
- [ ] Enable audit logging

### Performance
- [ ] Size hardware appropriately
- [ ] Configure connection limits
- [ ] Set resource limits
- [ ] Enable monitoring
- [ ] Configure persistence
- [ ] Test load capacity
- [ ] Plan for scaling

### Reliability
- [ ] Configure high availability
- [ ] Set up backups
- [ ] Plan disaster recovery
- [ ] Configure health checks
- [ ] Enable alerting
- [ ] Document procedures
- [ ] Test failover

## System Requirements

### Hardware Requirements

**Small Deployment (< 1,000 clients)**
- CPU: 2-4 cores
- RAM: 4-8 GB
- Storage: 50 GB SSD
- Network: 100 Mbps

**Medium Deployment (1,000-10,000 clients)**
- CPU: 4-8 cores
- RAM: 16-32 GB
- Storage: 200 GB SSD
- Network: 1 Gbps

**Large Deployment (10,000+ clients)**
- CPU: 8-16+ cores
- RAM: 32-64+ GB
- Storage: 500 GB+ SSD RAID
- Network: 10 Gbps

### Operating System

**Recommended**: Ubuntu 22.04 LTS or later

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install dependencies
sudo apt install -y build-essential pkg-config libssl-dev

# Increase file descriptor limits
echo "* soft nofile 65536" | sudo tee -a /etc/security/limits.conf
echo "* hard nofile 65536" | sudo tee -a /etc/security/limits.conf

# Network tuning
sudo sysctl -w net.core.somaxconn=65535
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=65535
sudo sysctl -w net.core.netdev_max_backlog=65535
```

## Installation

### Binary Installation

```bash
# Download latest release
wget https://github.com/fabriciobracht/mqtt-lib/releases/latest/download/mqtt-broker-linux-amd64.tar.gz

# Extract
tar -xzf mqtt-broker-linux-amd64.tar.gz

# Install
sudo mv mqtt-broker /usr/local/bin/
sudo chmod +x /usr/local/bin/mqtt-broker

# Create user
sudo useradd -r -s /bin/false mqtt

# Create directories
sudo mkdir -p /etc/mqtt /var/lib/mqtt /var/log/mqtt
sudo chown mqtt:mqtt /var/lib/mqtt /var/log/mqtt
```

### Building from Source

```bash
# Clone repository
git clone https://github.com/fabriciobracht/mqtt-lib.git
cd mqtt-lib

# Build release binary
cargo build --release --bin mqtt-broker

# Install
sudo cp target/release/mqtt-broker /usr/local/bin/
```

## Configuration

### Production Configuration File

Create `/etc/mqtt/broker.toml`:

```toml
# Network Configuration
bind_address = "0.0.0.0:1883"
max_clients = 10000
max_packet_size = 1048576  # 1MB
session_expiry_interval = 3600  # 1 hour

# MQTT Settings
maximum_qos = 2
retain_available = true
wildcard_subscription_available = true
subscription_identifier_available = true
shared_subscription_available = true
topic_alias_maximum = 100

# Authentication
[auth_config]
allow_anonymous = false
password_file = "/etc/mqtt/users.txt"
auth_method = "Password"

# TLS Configuration
[tls_config]
cert_file = "/etc/mqtt/certs/server.crt"
key_file = "/etc/mqtt/certs/server.key"
ca_file = "/etc/mqtt/certs/ca.crt"
require_client_cert = false
bind_address = "0.0.0.0:8883"

# WebSocket Configuration
[websocket_config]
bind_address = "0.0.0.0:8080"
path = "/mqtt"
subprotocol = "mqtt"
use_tls = false

# Storage Configuration
[storage_config]
backend = "File"
base_dir = "/var/lib/mqtt"
cleanup_interval = 3600
enable_persistence = true

# Resource Limits
[resource_limits]
max_connections = 10000
max_connections_per_ip = 100
max_memory_bytes = 2147483648  # 2GB
max_message_rate_per_client = 1000
max_bandwidth_per_client = 10485760  # 10MB/s
max_connection_rate = 100
rate_limit_window = 60
```

### TLS Certificate Setup

```bash
# Generate certificates (production should use CA-signed certs)
cd /etc/mqtt/certs

# For Let's Encrypt
sudo certbot certonly --standalone -d mqtt.example.com
sudo ln -s /etc/letsencrypt/live/mqtt.example.com/fullchain.pem server.crt
sudo ln -s /etc/letsencrypt/live/mqtt.example.com/privkey.pem server.key

# Set permissions
sudo chown -R mqtt:mqtt /etc/mqtt/certs
sudo chmod 600 /etc/mqtt/certs/*.key
```

### User Authentication

Create `/etc/mqtt/users.txt`:

```bash
# Generate password hashes
python3 -c "import bcrypt; print(bcrypt.hashpw(b'admin_password', bcrypt.gensalt()).decode())"

# Add to users.txt
echo "admin:$2b$12$..." | sudo tee /etc/mqtt/users.txt
echo "client1:$2b$12$..." | sudo tee -a /etc/mqtt/users.txt

# Secure the file
sudo chown mqtt:mqtt /etc/mqtt/users.txt
sudo chmod 600 /etc/mqtt/users.txt
```

## Systemd Service

Create `/etc/systemd/system/mqtt-broker.service`:

```ini
[Unit]
Description=MQTT v5.0 Broker
Documentation=https://github.com/fabriciobracht/mqtt-lib
After=network.target

[Service]
Type=simple
User=mqtt
Group=mqtt
ExecStart=/usr/local/bin/mqtt-broker --config /etc/mqtt/broker.toml
ExecReload=/bin/kill -HUP $MAINPID
Restart=on-failure
RestartSec=10

# Security
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/mqtt /var/log/mqtt

# Resource Limits
LimitNOFILE=65536
LimitCORE=0

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=mqtt-broker

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable mqtt-broker
sudo systemctl start mqtt-broker
sudo systemctl status mqtt-broker
```

## Logging Configuration

### Systemd Journal

```bash
# View logs
sudo journalctl -u mqtt-broker -f

# Export logs
sudo journalctl -u mqtt-broker --since "1 hour ago" > mqtt-broker.log
```

### Log Rotation

Create `/etc/logrotate.d/mqtt-broker`:

```
/var/log/mqtt/*.log {
    daily
    rotate 14
    compress
    delaycompress
    missingok
    notifempty
    create 0640 mqtt mqtt
    postrotate
        systemctl reload mqtt-broker > /dev/null 2>&1 || true
    endscript
}
```

## Monitoring Setup

### Prometheus Integration

Add to `/etc/mqtt/broker.toml`:

```toml
[monitoring]
prometheus_endpoint = "0.0.0.0:9090"
enable_sys_topics = true
sys_topic_interval = 10
```

### Health Check Script

Create `/usr/local/bin/mqtt-health-check`:

```bash
#!/bin/bash
# MQTT Broker Health Check

BROKER_HOST="${1:-localhost}"
BROKER_PORT="${2:-1883}"
TIMEOUT=5

# Try to connect
timeout $TIMEOUT mosquitto_pub \
    -h $BROKER_HOST \
    -p $BROKER_PORT \
    -t "health/check" \
    -m "ping" \
    -q 0 \
    2>/dev/null

if [ $? -eq 0 ]; then
    echo "OK - MQTT broker is responding"
    exit 0
else
    echo "CRITICAL - MQTT broker is not responding"
    exit 2
fi
```

## High Availability

### Active-Passive Setup

Primary broker configuration:

```toml
[ha_config]
role = "primary"
peer_address = "backup.example.com:1883"
sync_interval = 5
```

Backup broker configuration:

```toml
[ha_config]
role = "backup"
peer_address = "primary.example.com:1883"
sync_interval = 5
```

### Load Balancer Configuration

HAProxy configuration (`/etc/haproxy/haproxy.cfg`):

```
global
    maxconn 50000
    
defaults
    mode tcp
    timeout connect 5s
    timeout client 30s
    timeout server 30s

listen mqtt
    bind *:1883
    balance leastconn
    option tcplog
    option tcp-check
    
    server mqtt1 10.0.0.1:1883 check
    server mqtt2 10.0.0.2:1883 check
    server mqtt3 10.0.0.3:1883 check
```

## Security Hardening

### Firewall Rules

```bash
# Allow MQTT ports
sudo ufw allow 1883/tcp comment "MQTT"
sudo ufw allow 8883/tcp comment "MQTT/TLS"
sudo ufw allow 8080/tcp comment "MQTT/WebSocket"

# Restrict management access
sudo ufw allow from 10.0.0.0/8 to any port 22 comment "SSH from internal"
sudo ufw allow from 10.0.0.0/8 to any port 9090 comment "Metrics from internal"

# Enable firewall
sudo ufw --force enable
```

### SELinux Policy

```bash
# Create custom policy
sudo ausearch -c 'mqtt-broker' --raw | audit2allow -M mqtt-broker
sudo semodule -i mqtt-broker.pp

# Set contexts
sudo semanage fcontext -a -t bin_t '/usr/local/bin/mqtt-broker'
sudo restorecon -v '/usr/local/bin/mqtt-broker'
```

### AppArmor Profile

Create `/etc/apparmor.d/mqtt-broker`:

```
#include <tunables/global>

/usr/local/bin/mqtt-broker {
  #include <abstractions/base>
  #include <abstractions/nameservice>
  
  network inet stream,
  network inet6 stream,
  
  /usr/local/bin/mqtt-broker mr,
  /etc/mqtt/** r,
  /var/lib/mqtt/** rw,
  /var/log/mqtt/** rw,
  
  # Deny everything else
  deny /** w,
}
```

## Backup and Recovery

### Backup Script

Create `/usr/local/bin/mqtt-backup`:

```bash
#!/bin/bash
# MQTT Broker Backup Script

BACKUP_DIR="/backup/mqtt"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_NAME="mqtt-backup-${TIMESTAMP}"

# Create backup directory
mkdir -p "${BACKUP_DIR}/${BACKUP_NAME}"

# Stop broker for consistent backup
systemctl stop mqtt-broker

# Backup data
cp -r /var/lib/mqtt/* "${BACKUP_DIR}/${BACKUP_NAME}/"
cp -r /etc/mqtt/* "${BACKUP_DIR}/${BACKUP_NAME}/"

# Start broker
systemctl start mqtt-broker

# Compress backup
cd "${BACKUP_DIR}"
tar -czf "${BACKUP_NAME}.tar.gz" "${BACKUP_NAME}"
rm -rf "${BACKUP_NAME}"

# Remove old backups (keep 7 days)
find "${BACKUP_DIR}" -name "mqtt-backup-*.tar.gz" -mtime +7 -delete

echo "Backup completed: ${BACKUP_NAME}.tar.gz"
```

### Automated Backups

Add to crontab:

```bash
# Daily backup at 2 AM
0 2 * * * /usr/local/bin/mqtt-backup >> /var/log/mqtt-backup.log 2>&1
```

## Performance Tuning

### Kernel Parameters

Add to `/etc/sysctl.d/99-mqtt.conf`:

```bash
# Network performance
net.core.somaxconn = 65535
net.ipv4.tcp_max_syn_backlog = 65535
net.core.netdev_max_backlog = 65535
net.ipv4.tcp_fin_timeout = 15
net.ipv4.tcp_keepalive_time = 300
net.ipv4.tcp_keepalive_probes = 5
net.ipv4.tcp_keepalive_intvl = 15

# Memory
vm.swappiness = 10
vm.dirty_ratio = 15
vm.dirty_background_ratio = 5

# File handles
fs.file-max = 2097152
```

Apply:

```bash
sudo sysctl -p /etc/sysctl.d/99-mqtt.conf
```

### Process Limits

Add to `/etc/security/limits.d/mqtt.conf`:

```
mqtt soft nofile 65536
mqtt hard nofile 65536
mqtt soft nproc 32768
mqtt hard nproc 32768
```

## Monitoring and Alerting

### Nagios Check

```bash
#!/bin/bash
# /usr/lib/nagios/plugins/check_mqtt

BROKER=$1
PORT=${2:-1883}

# Check connection
timeout 5 mosquitto_sub -h $BROKER -p $PORT -t '$SYS/broker/uptime' -C 1 > /dev/null 2>&1

if [ $? -eq 0 ]; then
    echo "MQTT OK - Broker is responding"
    exit 0
else
    echo "MQTT CRITICAL - Broker not responding"
    exit 2
fi
```

### Prometheus Alerts

```yaml
groups:
  - name: mqtt_alerts
    rules:
      - alert: MqttBrokerDown
        expr: up{job="mqtt-broker"} == 0
        for: 5m
        annotations:
          summary: "MQTT Broker is down"
          
      - alert: MqttHighConnections
        expr: mqtt_broker_clients_connected > 9000
        for: 10m
        annotations:
          summary: "High number of MQTT connections"
          
      - alert: MqttHighMemory
        expr: mqtt_broker_memory_used_bytes > 1.5e+9
        for: 15m
        annotations:
          summary: "MQTT Broker high memory usage"
```

## Troubleshooting

### Common Issues

**Broker won't start**
```bash
# Check logs
sudo journalctl -u mqtt-broker -n 100

# Verify configuration
mqtt-broker --config /etc/mqtt/broker.toml --check

# Check permissions
ls -la /var/lib/mqtt /etc/mqtt
```

**High CPU usage**
```bash
# Profile the broker
perf top -p $(pgrep mqtt-broker)

# Check connection count
mosquitto_sub -h localhost -t '$SYS/broker/clients/connected' -C 1

# Monitor message rate
watch -n 1 'mosquitto_sub -h localhost -t "$SYS/broker/messages/sent" -C 1'
```

**Memory issues**
```bash
# Check memory usage
ps aux | grep mqtt-broker

# Analyze heap
gdb -p $(pgrep mqtt-broker) -ex "heap" -ex "quit"

# Check for memory leaks
valgrind --leak-check=full mqtt-broker
```

## Maintenance

### Regular Tasks

**Daily**
- Check logs for errors
- Monitor resource usage
- Verify backup completion

**Weekly**
- Review security logs
- Check certificate expiration
- Test failover procedure

**Monthly**
- Apply security updates
- Review and update ACLs
- Performance analysis
- Capacity planning

### Update Procedure

```bash
# 1. Download new version
wget https://github.com/fabriciobracht/mqtt-lib/releases/latest/download/mqtt-broker-linux-amd64.tar.gz

# 2. Backup current version
sudo cp /usr/local/bin/mqtt-broker /usr/local/bin/mqtt-broker.backup

# 3. Stop service
sudo systemctl stop mqtt-broker

# 4. Install new version
tar -xzf mqtt-broker-linux-amd64.tar.gz
sudo mv mqtt-broker /usr/local/bin/

# 5. Start service
sudo systemctl start mqtt-broker

# 6. Verify
sudo systemctl status mqtt-broker
mqtt-broker --version
```

## Production Checklist

### Before Going Live

- [ ] Security audit completed
- [ ] Load testing performed
- [ ] Backup procedures tested
- [ ] Monitoring configured
- [ ] Alerting enabled
- [ ] Documentation updated
- [ ] Team trained
- [ ] Disaster recovery plan tested
- [ ] Change management process defined
- [ ] SLA defined and agreed

### Post-Deployment

- [ ] Monitor performance metrics
- [ ] Review security logs
- [ ] Gather user feedback
- [ ] Document issues and resolutions
- [ ] Plan capacity upgrades
- [ ] Schedule security reviews